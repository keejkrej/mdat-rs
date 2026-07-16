use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::process::Command;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use clap::Args;
use mdat_input::types::{ImageInfo, MetadataPayload};
use mdat_input::{ensure_input_exists, resolve_reader_adapter};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, Notify};

pub mod assets;
pub use assets::{asset_router, AssetSource};

// Embed the frontend bundle at compile time. `build.rs` ensures
// `web/dist/index.html` exists (a placeholder if no bundle has been built),
// so this macro always finds a directory. The embedded dir is `'static`.
static EMBEDDED_WEB: include_dir::Dir<'static> =
    include_dir::include_dir!("$CARGO_MANIFEST_DIR/web/dist");

/// Pick the asset source: if the embedded bundle looks like a real frontend
/// build (has JS assets, not just the build.rs placeholder `index.html`),
/// serve it; otherwise serve the runtime placeholder page.
fn asset_source() -> AssetSource {
    let has_real_assets = EMBEDDED_WEB
        .find("**/*.js")
        .map(|mut it| it.next().is_some())
        .unwrap_or(false);
    if has_real_assets {
        AssetSource::Embedded(&EMBEDDED_WEB)
    } else {
        AssetSource::Placeholder
    }
}

pub type Result<T> = std::result::Result<T, ViewError>;

#[derive(Debug, Error)]
pub enum ViewError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Input(#[from] mdat_input::MdatError),
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Args)]
pub struct ViewArgs {
    pub path: std::path::PathBuf,
    #[arg(long)]
    pub idle_timeout: Option<u64>,
    #[arg(long)]
    pub no_open: bool,
}

pub fn detect_bind_addr() -> IpAddr {
    detect_bind_addr_with(ifaces())
}

fn detect_bind_addr_with(list: Vec<Iface>) -> IpAddr {
    if list.iter().any(|i| is_tailscale_iface(&i.name)) {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST)
    }
}

fn announce_host() -> String {
    if detect_bind_addr().is_unspecified() {
        announce_host_with(ifaces())
    } else {
        IpAddr::V4(Ipv4Addr::LOCALHOST).to_string()
    }
}

fn announce_host_with(list: Vec<Iface>) -> String {
    if let Some(ip) = tailscale_ipv4(&list) {
        return ip.to_string();
    }
    if let Some(ip) = primary_nonloopback_ipv4(&list) {
        return ip.to_string();
    }
    IpAddr::V4(Ipv4Addr::LOCALHOST).to_string()
}

fn tailscale_ipv4(list: &[Iface]) -> Option<Ipv4Addr> {
    list.iter()
        .find(|i| is_tailscale_iface(&i.name))
        .and_then(|i| i.ipv4)
}

fn primary_nonloopback_ipv4(list: &[Iface]) -> Option<Ipv4Addr> {
    list.iter()
        .find_map(|i| i.ipv4.filter(|ip| !ip.is_loopback() && !ip.is_unspecified()))
}

fn is_tailscale_iface(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "tailscale0"
        || (name.starts_with("utun")
            && name.len() > 4
            && name[4..].chars().all(|c| c.is_ascii_digit()))
}

#[derive(Debug, Clone)]
pub struct Iface {
    pub name: String,
    pub ipv4: Option<Ipv4Addr>,
}

#[cfg(unix)]
fn ifaces() -> Vec<Iface> {
    use std::process::Command;
    for bin in ["ip", "ifconfig"] {
        let args: Vec<&str> = if bin == "ip" { vec!["addr"] } else { vec![] };
        let Ok(output) = Command::new(bin).args(&args).output() else {
            continue;
        };
        let text = String::from_utf8_lossy(&output.stdout);
        let list = parse_ifaces(&text, bin);
        if !list.is_empty() {
            return list;
        }
    }
    Vec::new()
}

#[cfg(not(unix))]
fn ifaces() -> Vec<Iface> {
    Vec::new()
}

#[cfg(unix)]
fn parse_ifaces(text: &str, bin: &str) -> Vec<Iface> {
    let mut out: Vec<Iface> = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if let Some(name) = parse_iface_line(line, bin) {
            if !name.is_empty() {
                current = Some(name.clone());
                out.push(Iface { name, ipv4: None });
                continue;
            }
        }
        if let Some(ip) = parse_inet_ipv4(line) {
            if let Some(name) = &current {
                if let Some(iface) = out.iter_mut().rev().find(|i| &i.name == name) {
                    if iface.ipv4.is_none() {
                        iface.ipv4 = Some(ip);
                    }
                }
            }
        }
    }
    out
}

#[cfg(unix)]
fn parse_inet_ipv4(line: &str) -> Option<Ipv4Addr> {
    let trimmed = line.trim_start();
    let after = trimmed.strip_prefix("inet ").or_else(|| trimmed.strip_prefix("inet addr:"))?;
    let token = after.split_whitespace().next()?;
    let host = token.split('/').next().or_else(|| token.split(' ').next())?;
    host.parse().ok()
}

#[cfg(unix)]
fn parse_iface_line(line: &str, bin: &str) -> Option<String> {
    let trimmed = line.trim_start();
    if bin == "ip" {
        let rest = trimmed
            .strip_prefix(|c: char| c.is_ascii_digit())
            .map(|r| r.trim_start_matches(':'))
            .unwrap_or(trimmed);
        rest.split(':').next().map(|s| s.trim().to_string())
    } else {
        trimmed.split(':').next().map(|s| s.trim().to_string())
    }
}

pub fn pick_free_port(bind_addr: IpAddr) -> std::io::Result<(IpAddr, u16)> {
    let listener = TcpListener::bind(SocketAddr::new(bind_addr, 0))?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok((bind_addr, port))
}

pub fn generate_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

enum SessionCmd {
    ReadFrame {
        p: usize,
        t: usize,
        c: usize,
        z: usize,
        reply: oneshot::Sender<mdat_input::Result<Vec<u16>>>,
    },
}

struct SendableSession(mdat_input::types::ReaderSession);
// SAFETY: The ReaderSession holds `Box<dyn FnMut>` closures that may capture
// non-Send types (e.g. ND2 SDK handles). We assert Send here because the session
// is pinned to a single dedicated OS thread (see `SessionHandle::spawn`): it is
// created on that thread, all `read_frame` calls happen there, and only the
// `Send` result (`Vec<u16>` / `MdatError`) crosses the channel back. The session
// never escapes its owning thread, so there is no concurrent access.
unsafe impl Send for SendableSession {}

impl SendableSession {
    fn read_frame(
        &mut self,
        p: usize,
        t: usize,
        c: usize,
        z: usize,
    ) -> mdat_input::Result<Vec<u16>> {
        self.0.read_frame(p, t, c, z)
    }
}

struct SessionHandle {
    info: ImageInfo,
    width: u32,
    height: u32,
    tx: StdMutex<Option<mpsc::UnboundedSender<SessionCmd>>>,
}

impl SessionHandle {
    fn spawn(session: mdat_input::types::ReaderSession) -> Arc<Self> {
        let info = session.info;
        let width = session.width;
        let height = session.height;
        let (tx, mut rx) = mpsc::unbounded_channel::<SessionCmd>();
        let sendable = SendableSession(session);
        std::thread::Builder::new()
            .name("mdat-view-reader".into())
            .spawn(move || {
                let mut session = sendable;
                while let Some(cmd) = rx.blocking_recv() {
                    match cmd {
                        SessionCmd::ReadFrame { p, t, c, z, reply } => {
                            let res = session.read_frame(p, t, c, z);
                            let _ = reply.send(res);
                        }
                    }
                }
                let _ = session.0.close();
            })
            .expect("spawn reader thread");
        Arc::new(Self {
            info,
            width,
            height,
            tx: StdMutex::new(Some(tx)),
        })
    }

    async fn read_frame(
        &self,
        p: usize,
        t: usize,
        c: usize,
        z: usize,
    ) -> mdat_input::Result<Vec<u16>> {
        let (reply_tx, reply_rx) = oneshot::channel();
        let tx = self.tx.lock().unwrap().clone().ok_or_else(|| {
            mdat_input::MdatError::InvalidInput("reader session closed".to_string())
        })?;
        tx.send(SessionCmd::ReadFrame {
            p,
            t,
            c,
            z,
            reply: reply_tx,
        })
        .map_err(|_| {
            mdat_input::MdatError::InvalidInput("reader thread exited".to_string())
        })?;
        reply_rx
            .await
            .map_err(|_| {
                mdat_input::MdatError::InvalidInput("reader thread dropped reply".to_string())
            })
            .and_then(|r| r)
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        if let Some(tx) = self.tx.lock().unwrap().take() {
            drop(tx);
        }
    }
}

#[derive(Clone)]
struct ServerState {
    session: Arc<SessionHandle>,
    name: Arc<String>,
    metadata: Arc<MetadataPayload>,
    last_activity: Arc<StdMutex<Instant>>,
}

#[derive(Debug, Serialize)]
struct DatasetResp {
    name: String,
    info: DatasetInfo,
    width: u32,
    height: u32,
    channels: Vec<ChannelResp>,
    metadata: MetadataResp,
}

#[derive(Debug, Serialize)]
struct DatasetInfo {
    n_pos: usize,
    n_time: usize,
    n_chan: usize,
    n_z: usize,
}

#[derive(Debug, Serialize)]
struct ChannelResp {
    index: usize,
    name: String,
    color: String,
}

#[derive(Debug, Serialize)]
struct MetadataResp {
    normalized: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaneQuery {
    p: Option<usize>,
    t: Option<usize>,
    c: Option<usize>,
    z: Option<usize>,
}

#[derive(Debug, Serialize)]
struct ErrorResp {
    error: String,
}

#[derive(Debug, Serialize)]
struct FrameErrorResp {
    error: String,
    frame: FrameAddr,
}

#[derive(Debug, Serialize)]
struct FrameAddr {
    p: usize,
    t: usize,
    c: usize,
    z: usize,
}

pub async fn run(args: ViewArgs) -> Result<()> {
    let url = serve(&args, true).await?;
    if !args.no_open {
        open_browser(&url);
    }
    Ok(())
}

async fn serve(args: &ViewArgs, announce: bool) -> Result<String> {
    let setup = setup_server(args).await?;
    if announce {
        println!("mdat view serving {} at {}", setup.name, setup.base_url);
    }
    let url = setup.base_url.clone();
    run_server(setup).await?;
    Ok(url)
}

pub struct ServerSetup {
    pub base_url: String,
    pub name: String,
    listener: tokio::net::TcpListener,
    app: Router,
    idle_handle: tokio::task::JoinHandle<()>,
    pub idle_shutdown: Arc<Notify>,
}

async fn setup_server(args: &ViewArgs) -> Result<ServerSetup> {
    ensure_input_exists(&args.path)?;
    let adapter = resolve_reader_adapter(&args.path)?;
    let metadata = adapter.inspect_metadata(&args.path)?;
    let session = adapter.open(&args.path)?;
    let session = SessionHandle::spawn(session);

    let bind = detect_bind_addr();
    let listener = tokio::net::TcpListener::bind(SocketAddr::new(bind, 0)).await?;
    let local = listener.local_addr()?;
    let port = local.port();

    let token = generate_token();
    let name = args
        .path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| args.path.display().to_string());

    let idle_minutes = args.idle_timeout.unwrap_or(30);
    let idle_duration = Duration::from_secs(idle_minutes.saturating_mul(60));

    let last_activity = Arc::new(StdMutex::new(Instant::now()));

    let state = ServerState {
        session,
        name: Arc::new(name.clone()),
        metadata: Arc::new(metadata),
        last_activity: last_activity.clone(),
    };

    let asset_src = std::sync::Arc::new(asset_source());
    let app = Router::new()
        .route(&format!("/{token}/dataset"), get(get_dataset))
        .route(&format!("/{token}/plane"), get(get_plane))
        .with_state(state)
        .merge(assets::asset_router_with(asset_src.clone()))
        .fallback(assets::asset_fallback(asset_src));

    let base_url = format!("http://{announce}:{port}/{token}/", announce = announce_host());

    let idle_shutdown = Arc::new(Notify::new());
    let idle_handle = {
        let idle_shutdown = idle_shutdown.clone();
        let last_for_task = last_activity.clone();
        tokio::spawn(async move {
            if idle_duration.is_zero() {
                idle_shutdown.notify_one();
                return;
            }
            loop {
                tokio::time::sleep(idle_duration).await;
                let last = *last_for_task.lock().unwrap();
                if last.elapsed() >= idle_duration {
                    println!("shutting down (idle)");
                    idle_shutdown.notify_one();
                    return;
                }
            }
        })
    };

    Ok(ServerSetup {
        base_url,
        name,
        listener,
        app,
        idle_handle,
        idle_shutdown,
    })
}

async fn run_server(setup: ServerSetup) -> Result<()> {
    let ServerSetup {
        listener,
        app,
        idle_handle,
        idle_shutdown,
        ..
    } = setup;

    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    let serve_fut = axum::serve(listener, app).with_graceful_shutdown(async move {
        tokio::select! {
            _ = ctrl_c => {}
            _ = idle_shutdown.notified() => {}
        }
    });

    serve_fut.await?;
    idle_handle.abort();
    Ok(())
}

#[doc(hidden)]
pub async fn __test_setup(args: &ViewArgs) -> Result<ServerSetup> {
    setup_server(args).await
}

#[doc(hidden)]
pub async fn __test_run(setup: ServerSetup) -> Result<()> {
    run_server(setup).await
}

async fn get_dataset(State(state): State<ServerState>) -> Response {
    let info = state.session.info;
    let width = state.session.width;
    let height = state.session.height;
    let channels = extract_channels(&state.metadata.normalized, info.n_chan);
    let resp = DatasetResp {
        name: state.name.as_ref().clone(),
        info: DatasetInfo {
            n_pos: info.n_pos,
            n_time: info.n_time,
            n_chan: info.n_chan,
            n_z: info.n_z,
        },
        width,
        height,
        channels,
        metadata: MetadataResp {
            normalized: state.metadata.normalized.clone(),
            raw: state.metadata.raw.clone(),
            raw_format: state.metadata.raw_format.clone(),
        },
    };

    touch_activity(&state.last_activity);
    json_ok(&resp)
}

async fn get_plane(
    State(state): State<ServerState>,
    Query(q): Query<PlaneQuery>,
) -> Response {
    let p = q.p.unwrap_or(0);
    let t = q.t.unwrap_or(0);
    let c = q.c.unwrap_or(0);
    let z = q.z.unwrap_or(0);

    let info = state.session.info;
    let width = state.session.width;
    let height = state.session.height;

    touch_activity(&state.last_activity);

    if p >= info.n_pos || t >= info.n_time || c >= info.n_chan || z >= info.n_z {
        return json_err(
            StatusCode::BAD_REQUEST,
            &ErrorResp {
                error: format!(
                    "index out of range: p={p} (0..{npos}), t={t} (0..{ntime}), c={c} (0..{nchan}), z={z} (0..{nz})",
                    npos = info.n_pos,
                    ntime = info.n_time,
                    nchan = info.n_chan,
                    nz = info.n_z
                ),
            },
        );
    }

    let expected_bytes = (width as usize) * (height as usize) * 2;

    match state.session.read_frame(p, t, c, z).await {
        Ok(frame) => {
            if frame.len() * 2 != expected_bytes {
                return json_frame_err(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &FrameErrorResp {
                        error: format!(
                            "frame length mismatch: got {} u16 ({} bytes), expected {expected_bytes} bytes",
                            frame.len(),
                            frame.len() * 2
                        ),
                        frame: FrameAddr { p, t, c, z },
                    },
                );
            }
            let mut bytes = Vec::with_capacity(expected_bytes);
            for px in &frame {
                bytes.extend_from_slice(&px.to_le_bytes());
            }
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/octet-stream")],
                bytes,
            )
                .into_response()
        }
        Err(e) => json_frame_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            &FrameErrorResp {
                error: e.to_string(),
                frame: FrameAddr { p, t, c, z },
            },
        ),
    }
}

fn touch_activity(last: &Arc<StdMutex<Instant>>) {
    let mut g = last.lock().unwrap();
    *g = Instant::now();
}

fn json_ok<T: Serialize>(body: &T) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_vec(body).unwrap(),
    )
        .into_response()
}

fn json_err<T: Serialize>(code: StatusCode, body: &T) -> Response {
    (
        code,
        [(header::CONTENT_TYPE, "application/json")],
        serde_json::to_vec(body).unwrap(),
    )
        .into_response()
}

fn json_frame_err<T: Serialize>(code: StatusCode, body: &T) -> Response {
    json_err(code, body)
}

fn extract_channels(normalized: &Value, n_chan: usize) -> Vec<ChannelResp> {
    let arr = normalized
        .get("channels")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    (0..n_chan)
        .map(|i| {
            let entry = arr.get(i);
            let name = entry
                .and_then(|c| c.get("name"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Channel {i}"));
            let color = entry
                .and_then(|c| c.get("color"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| "#ffffff".to_string());
            let index = entry
                .and_then(|c| c.get("index"))
                .and_then(|v| v.as_u64())
                .map(|v| v as usize)
                .unwrap_or(i);
            ChannelResp { index, name, color }
        })
        .collect()
}

fn open_browser(url: &str) {
    let (bin, args): (&str, Vec<String>) = if cfg!(target_os = "macos") {
        ("open", vec![url.to_string()])
    } else if cfg!(target_os = "windows") {
        (
            "cmd",
            vec!["/c".to_string(), "start".to_string(), url.to_string()],
        )
    } else {
        ("xdg-open", vec![url.to_string()])
    };
    let _ = Command::new(bin).args(&args).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn iface(name: &str) -> Iface {
        Iface { name: name.to_string(), ipv4: None }
    }

    #[test]
    fn bind_localhost_without_tailscale() {
        assert_eq!(
            detect_bind_addr_with(vec![iface("eth0"), iface("lo")]),
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        );
    }

    #[test]
    fn bind_unspecified_with_tailscale0() {
        assert_eq!(
            detect_bind_addr_with(vec![iface("eth0"), iface("tailscale0")]),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
    }

    #[test]
    fn bind_unspecified_with_utun4() {
        assert_eq!(
            detect_bind_addr_with(vec![iface("en0"), iface("utun4")]),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        );
    }

    #[test]
    fn bind_localhost_empty() {
        assert_eq!(detect_bind_addr_with(vec![]), IpAddr::V4(Ipv4Addr::LOCALHOST));
    }

    #[test]
    fn parse_ip_addr_line_extracts_name() {
        assert_eq!(parse_iface_line("2: tailscale0: <NO-CARRIER,...>", "ip"), Some("tailscale0".into()));
        assert_eq!(parse_iface_line("1: lo: <LOOPBACK,...>", "ip"), Some("lo".into()));
        assert_eq!(parse_iface_line("3: en0: <BROADCAST,...>", "ip"), Some("en0".into()));
    }

    #[test]
    fn parse_ifconfig_line_extracts_name() {
        assert_eq!(parse_iface_line("utun4: flags=8051<UP,POINTOPOINT,...>", "ifconfig"), Some("utun4".into()));
        assert_eq!(parse_iface_line("en0: flags=8863<UP,...>", "ifconfig"), Some("en0".into()));
        assert_eq!(parse_iface_line("lo0: flags=8049<UP,LOOPBACK,...>", "ifconfig"), Some("lo0".into()));
    }

    #[test]
    fn tailscale_iface_detection_covers_utun_variants() {
        assert!(is_tailscale_iface("tailscale0"));
        assert!(is_tailscale_iface("utun4"));
        assert!(is_tailscale_iface("utun0"));
        assert!(is_tailscale_iface("utun12"));
        assert!(!is_tailscale_iface("en0"));
        assert!(!is_tailscale_iface("utun"));
        assert!(!is_tailscale_iface("utunX"));
    }

    #[test]
    fn announce_host_picks_tailscale_ipv4_when_present() {
        let ifaces = vec![
            Iface { name: "eth0".into(), ipv4: Some(Ipv4Addr::new(192, 168, 1, 10)) },
            Iface { name: "tailscale0".into(), ipv4: Some(Ipv4Addr::new(100, 64, 0, 1)) },
        ];
        assert_eq!(announce_host_with(ifaces), "100.64.0.1");
    }

    #[test]
    fn announce_host_falls_back_to_nonloopback_ipv4() {
        let ifaces = vec![
            Iface { name: "lo".into(), ipv4: Some(Ipv4Addr::new(127, 0, 0, 1)) },
            Iface { name: "eth0".into(), ipv4: Some(Ipv4Addr::new(192, 168, 1, 10)) },
        ];
        assert_eq!(announce_host_with(ifaces), "192.168.1.10");
    }

    #[test]
    fn announce_host_falls_back_to_localhost() {
        assert_eq!(announce_host_with(vec![]), "127.0.0.1");
    }

    #[test]
    fn free_port_is_nonzero_and_matches_bind() {
        let (addr, port) = pick_free_port(IpAddr::V4(Ipv4Addr::LOCALHOST)).unwrap();
        assert_eq!(addr, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert!(port > 0);
    }

    #[test]
    fn token_is_22_chars_urlsafe() {
        let token = generate_token();
        assert_eq!(token.len(), 22);
        assert!(token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    }

    #[test]
    fn token_is_not_constant() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
    }

    #[test]
    fn extract_channels_reads_normalized_array() {
        let normalized = json!({
            "channels": [
                {"index": 0, "name": "DAPI", "color": "#0000ff"},
                {"index": 1, "name": "GFP", "color": "#00ff00"},
            ]
        });
        let ch = extract_channels(&normalized, 2);
        assert_eq!(ch.len(), 2);
        assert_eq!(ch[0].name, "DAPI");
        assert_eq!(ch[0].color, "#0000ff");
        assert_eq!(ch[1].index, 1);
    }

    #[test]
    fn extract_channels_fills_defaults_when_missing() {
        let normalized = json!({});
        let ch = extract_channels(&normalized, 1);
        assert_eq!(ch.len(), 1);
        assert_eq!(ch[0].name, "Channel 0");
        assert_eq!(ch[0].color, "#ffffff");
        assert_eq!(ch[0].index, 0);
    }
}