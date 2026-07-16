use std::fs::File;
use std::path::Path;
use std::time::Duration;

use mdat_view::__test_run;
use mdat_view::__test_setup;
use mdat_view::ViewArgs;
use serde_json::Value;
use tempfile::tempdir;
use tiff::encoder::{colortype, TiffEncoder};

const W: u32 = 4;
const H: u32 = 3;

fn write_tiff(path: &Path, pixels: &[u16], width: u32, height: u32) {
    if path.exists() {
        std::fs::remove_file(path).unwrap();
    }
    let file = File::create(path).unwrap();
    let mut encoder = TiffEncoder::new(file).unwrap();
    let image = encoder.new_image::<colortype::Gray16>(width, height).unwrap();
    image.write_data(pixels).unwrap();
}

fn make_frame(seed: u16) -> Vec<u16> {
    (0..(W * H)).map(|i| (seed + i as u16) % 1000).collect()
}

fn write_mdat_tree(root: &Path, n_pos: usize, n_time: usize, n_chan: usize, n_z: usize) {
    for p in 0..n_pos {
        let pos_dir = root.join(format!("Pos{p}"));
        std::fs::create_dir_all(&pos_dir).unwrap();
        let mut time_map = String::from("t,t_real\n");
        for t in 0..n_time {
            time_map.push_str(&format!("{t},{t}\n"));
        }
        std::fs::write(pos_dir.join("time_map.csv"), time_map).unwrap();
        for t in 0..n_time {
            for c in 0..n_chan {
                for z in 0..n_z {
                    let frame = make_frame(((p * 100 + t * 10 + c * 3 + z) as u16) * 7);
                    let filename =
                        format!("img_channel{c:03}_position{p:03}_time{t:09}_z{z:03}.tif");
                    write_tiff(&pos_dir.join(filename), &frame, W, H);
                }
            }
        }
    }
}

async fn boot_server(path: &Path, idle_timeout: Option<u64>) -> (mdat_view::ServerSetup, String) {
    let args = ViewArgs {
        path: path.to_path_buf(),
        idle_timeout,
        no_open: true,
    };
    let setup = __test_setup(&args).await.expect("setup");
    let base = setup.base_url.clone();
    let base = base.replacen("://0.0.0.0:", "://127.0.0.1:", 1);
    (setup, base)
}

fn view_args(path: &Path, idle_timeout: Option<u64>) -> ViewArgs {
    ViewArgs {
        path: path.to_path_buf(),
        idle_timeout,
        no_open: true,
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn dataset_returns_contract_json() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 2, 3, 2, 2);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let resp = client().get(format!("{base}dataset")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["info"]["n_pos"], 2);
    assert_eq!(body["info"]["n_time"], 3);
    assert_eq!(body["info"]["n_chan"], 2);
    assert_eq!(body["info"]["n_z"], 2);
    assert_eq!(body["width"], W);
    assert_eq!(body["height"], H);
    let channels = body["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 2);
    assert_eq!(channels[0]["color"], "#ff0000");
    assert_eq!(channels[1]["color"], "#00ff00");
    assert!(body["metadata"]["normalized"]["channels"].is_array());

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn plane_returns_octet_stream_of_expected_size() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let resp = client().get(format!("{base}plane")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.headers().get(reqwest::header::CONTENT_TYPE).unwrap(),
        "application/octet-stream"
    );
    let bytes = resp.bytes().await.unwrap();
    assert_eq!(bytes.len(), (W * H * 2) as usize);
    let expected = make_frame(0);
    let mut got = Vec::with_capacity((W * H) as usize);
    for chunk in bytes.chunks_exact(2) {
        got.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }
    assert_eq!(got, expected);

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn plane_with_query_params_reads_correct_frame() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 2, 1, 1);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let t0 = client()
        .get(format!("{base}plane?t=0"))
        .send()
        .await
        .unwrap();
    let t1 = client()
        .get(format!("{base}plane?t=1"))
        .send()
        .await
        .unwrap();
    assert_eq!(t0.status(), 200);
    assert_eq!(t1.status(), 200);
    let b0 = t0.bytes().await.unwrap();
    let b1 = t1.bytes().await.unwrap();
    assert_ne!(b0.as_ref(), b1.as_ref());
    assert_eq!(b0.len(), (W * H * 2) as usize);

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn wrong_token_returns_404() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let host = base.trim_start_matches("http://").split('/').next().unwrap();
    let bad_url = format!("http://{host}/wrongtoken/dataset");
    let resp = client().get(&bad_url).send().await.unwrap();
    assert_eq!(resp.status(), 404);

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn out_of_range_returns_400_with_error() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let resp = client()
        .get(format!("{base}plane?t=99"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("out of range"));

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn read_failure_returns_500_with_frame() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let file = dir
        .path()
        .join("Pos0")
        .join("img_channel000_position000_time000000000_z000.tif");

    let (setup, base) = boot_server(dir.path(), None).await;
    std::fs::write(&file, b"not a real tiff anymore").unwrap();
    let server = tokio::spawn(__test_run(setup));

    let resp = client().get(format!("{base}plane")).send().await.unwrap();
    assert_eq!(resp.status(), 500);
    let body: Value = resp.json().await.unwrap();
    assert!(body["error"].as_str().is_some());
    assert_eq!(body["frame"]["p"], 0);
    assert_eq!(body["frame"]["t"], 0);
    assert_eq!(body["frame"]["c"], 0);
    assert_eq!(body["frame"]["z"], 0);

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_timeout_exits_server_when_idle() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let (setup, _) = boot_server(dir.path(), Some(0)).await;
    let server = tokio::spawn(__test_run(setup));

    let finished = tokio::time::timeout(Duration::from_secs(3), server).await;
    assert!(finished.is_ok(), "server should exit when idle timeout is 0");
}

#[tokio::test(flavor = "multi_thread")]
async fn root_serves_html() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let host = base.trim_start_matches("http://").split('/').next().unwrap();
    let resp = client()
        .get(format!("http://{host}/"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let ctype = resp
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap())
        .unwrap_or("");
    assert!(
        ctype.starts_with("text/html"),
        "root should serve HTML (placeholder or bundle), got content-type {ctype}"
    );
    let body = resp.text().await.unwrap();
    // Either the placeholder ("build the frontend") or the real bundle
    // (contains a root div + module script). Both are valid depending on
    // whether the frontend has been built in this environment.
    assert!(
        body.contains("build the frontend") || body.contains("id=\"root\""),
        "root should serve the placeholder or the app shell"
    );

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn multi_position_dataset_reports_n_pos_and_serves_each() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 2, 2, 2, 1);
    let (setup, base) = boot_server(dir.path(), None).await;
    let server = tokio::spawn(__test_run(setup));

    let resp = client().get(format!("{base}dataset")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await.unwrap();
    assert_eq!(body["info"]["n_pos"], 2);

    for p in 0..2 {
        let resp = client()
            .get(format!("{base}plane?p={p}&t=0&c=0&z=0"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "plane p={p} should be in range");
        let bytes = resp.bytes().await.unwrap();
        assert_eq!(bytes.len(), (W * H * 2) as usize);
    }

    let oob = client()
        .get(format!("{base}plane?p=2&t=0&c=0&z=0"))
        .send()
        .await
        .unwrap();
    assert_eq!(oob.status(), 400);

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn request_keeps_idle_server_alive_past_window() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);

    let (setup, base) = boot_server(dir.path(), None).await;
    let idle_shutdown = setup.idle_shutdown.clone();
    let server = tokio::spawn(__test_run(setup));

    let resp = client().get(format!("{base}dataset")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    drop(resp);

    let notified = tokio::time::timeout(Duration::from_millis(300), idle_shutdown.notified()).await;
    assert!(
        notified.is_err(),
        "with a long default idle timeout, server should not exit within 300ms after a request"
    );

    server.abort();
    let _ = server.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_timeout_zero_exits_promptly() {
    let dir = tempdir().unwrap();
    write_mdat_tree(dir.path(), 1, 1, 1, 1);
    let (setup, _) = boot_server(dir.path(), Some(0)).await;
    let server = tokio::spawn(__test_run(setup));

    let finished = tokio::time::timeout(Duration::from_secs(3), server).await;
    assert!(
        finished.is_ok(),
        "server should self-exit when idle-timeout is 0"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_nd2_file_fails_setup_without_panic() {
    let dir = tempdir().unwrap();
    let nd2 = dir.path().join("synthetic.nd2");
    std::fs::write(&nd2, b"this is not a real ND2 file").unwrap();

    let args = view_args(&nd2, None);
    let result = __test_setup(&args).await;
    assert!(
        result.is_err(),
        "fake ND2 should fail setup_server, not boot a server"
    );
    let err = match result { Err(e) => e.to_string(), Ok(_) => panic!("expected error") };
    assert!(
        !err.contains("panicked"),
        "failure should be a clean error, not a panic: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_czi_file_fails_setup_without_panic() {
    let dir = tempdir().unwrap();
    let czi = dir.path().join("synthetic.czi");
    std::fs::write(&czi, b"this is not a real CZI file").unwrap();

    let args = view_args(&czi, None);
    let result = __test_setup(&args).await;
    assert!(
        result.is_err(),
        "fake CZI should fail setup_server, not boot a server"
    );
    let err = match result { Err(e) => e.to_string(), Ok(_) => panic!("expected error") };
    assert!(
        !err.contains("panicked"),
        "failure should be a clean error, not a panic: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn nd2_suffix_dispatches_to_nd2_adapter_at_resolve_time() {
    use mdat_input::resolve_reader_adapter;
    let dir = tempdir().unwrap();
    let nd2 = dir.path().join("anything.nd2");
    std::fs::write(&nd2, b"x").unwrap();
    let adapter = resolve_reader_adapter(&nd2).expect("resolve ok");
    assert_eq!(adapter.name(), "nd2");
}

#[tokio::test(flavor = "multi_thread")]
async fn czi_suffix_dispatches_to_czi_adapter_at_resolve_time() {
    use mdat_input::resolve_reader_adapter;
    let dir = tempdir().unwrap();
    let czi = dir.path().join("anything.czi");
    std::fs::write(&czi, b"x").unwrap();
    let adapter = resolve_reader_adapter(&czi).expect("resolve ok");
    assert_eq!(adapter.name(), "czi");
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_suffix_is_unsupported_via_setup() {
    let dir = tempdir().unwrap();
    let bin = dir.path().join("foo.bin");
    std::fs::write(&bin, b"x").unwrap();
    let args = view_args(&bin, None);
    let result = __test_setup(&args).await;
    assert!(result.is_err());
    let err = match result { Err(e) => e.to_string(), Ok(_) => panic!("expected error") };
    assert!(err.contains(".bin") || err.contains("unsupported"), "err: {err}");
}