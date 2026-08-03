import {
  createEffect,
  createMemo,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
  type Component,
} from "solid-js";
import type {
  ChannelState,
  Dataset,
  Plane,
  PlaneAddr,
  ViewerError,
} from "./types";
import { fetchPlane, PlaneFetchError } from "./api";
import { PlaneCache } from "./viewer/colors";
import { composite, defaultContrast, percentileContrast } from "./viewer/composite";
import ChannelPanel from "./components/ChannelPanel";
import MetadataPanel from "./components/MetadataPanel";
import PixelInspector from "./components/PixelInspector";
import ErrorOverlay from "./components/ErrorOverlay";

interface Props {
  dataset: Dataset;
  token: string;
  onFatal: () => void;
}

const ZOOM_MIN = 0.05;
const ZOOM_MAX = 40;

const Viewer: Component<Props> = (props) => {
  const ds = () => props.dataset;

  // --- viewer state -------------------------------------------------------
  const cache = new PlaneCache(256);

  const [pos, setPos] = createSignal(0);
  const [t, setT] = createSignal(0);
  const [z, setZ] = createSignal(0);
  const [channels, setChannels] = createSignal<ChannelState[]>(
    ds().channels.map((c) => ({
      index: c.index,
      name: c.name,
      color: c.color,
      visible: true,
      contrastMin: defaultContrast().min,
      contrastMax: defaultContrast().max,
    })),
  );
  const [zoom, setZoom] = createSignal(1);
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  const [fit, setFit] = createSignal(true);
  const [error, setError] = createSignal<ViewerError | null>(null);
  const [pixel, setPixel] = createSignal<{
    x: number | null;
    y: number | null;
    values: { channel: ChannelState; value: number }[];
  }>({ x: null, y: null, values: [] });
  const [metaCollapsed, setMetaCollapsed] = createSignal(true);
  const [rendering, setRendering] = createSignal(false);

  let canvas: HTMLCanvasElement | undefined;
  let container: HTMLDivElement | undefined;
  let pendingTimer: ReturnType<typeof setTimeout> | null = null;
  let dragging: { startX: number; startY: number; panX: number; panY: number } | null = null;

  const enabledChannels = createMemo(() =>
    channels().filter((c) => c.visible),
  );

  // --- fit-to-window ------------------------------------------------------
  const computeFit = () => {
    const c = container;
    const d = ds();
    if (!c || !d) return 1;
    const availW = c.clientWidth - 4;
    const availH = c.clientHeight - 4;
    if (availW <= 0 || availH <= 0) return 1;
    const sx = availW / d.width;
    const sy = availH / d.height;
    return Math.min(sx, sy);
  };

  // --- plane fetch --------------------------------------------------------
  async function getPlane(addr: PlaneAddr): Promise<Plane> {
    const cached = cache.get(addr);
    if (cached) return cached;
    const plane = await fetchPlane(props.token, addr);
    cache.set(addr, plane);
    return plane;
  }

  /** The set of planes needed for the current frame, one per enabled channel. */
  function neededAddrs(): PlaneAddr[] {
    const p = pos();
    const tt = t();
    const zz = z();
    return enabledChannels().map((ch) => ({
      p,
      t: tt,
      c: ch.index,
      z: zz,
    }));
  }

  /** Fetch + composite + render the current frame. */
  async function renderCurrent(): Promise<void> {
    setRendering(true);
    try {
      const addrs = neededAddrs();
      if (addrs.length === 0) {
        if (canvas) {
          const ctx = canvas.getContext("2d");
          if (ctx && canvas.width && canvas.height) {
            ctx.clearRect(0, 0, canvas.width, canvas.height);
          }
        }
        setError(null);
        return;
      }
      const planes = await Promise.all(
        addrs.map(async (addr): Promise<{ addr: PlaneAddr; plane?: Plane; err?: unknown }> => {
          try {
            const plane = await getPlane(addr);
            return { addr, plane };
          } catch (e) {
            return { addr, err: e };
          }
        }),
      );
      // Surface the first failure as an error overlay.
      const firstErr = planes.find((p) => p.err !== undefined);
      if (firstErr && firstErr.err !== undefined) {
        const e = firstErr.err;
        if (e instanceof PlaneFetchError) {
          setError({ message: e.message, fatal: false, frame: e.frame });
        } else if (e instanceof Error) {
          setError({
            message: "could not fetch a plane — " + e.message,
            fatal: true,
          });
        } else {
          setError({ message: String(e), fatal: false });
        }
        return;
      }
      setError(null);
      const okPlanes = planes
        .filter((p): p is { addr: PlaneAddr; plane: Plane } => p.plane !== undefined)
        .map(({ addr, plane }) => {
          const ch = enabledChannels().find((c) => c.index === addr.c)!;
          return { channel: ch, plane };
        });
      drawComposite(okPlanes);
    } finally {
      setRendering(false);
    }
  }

  function drawComposite(
    planes: { channel: ChannelState; plane: Plane }[],
  ) {
    if (!canvas) return;
    const d = ds();
    canvas.width = d.width;
    canvas.height = d.height;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    if (planes.length === 0) {
      ctx.clearRect(0, 0, d.width, d.height);
      return;
    }
    const rgba = composite(planes, d.width, d.height);
    const imageData = new ImageData(
      rgba as Uint8ClampedArray<ArrayBuffer>,
      d.width,
      d.height,
    );
    ctx.putImageData(imageData, 0, 0);
  }

  // --- prefetch ring ------------------------------------------------------
  function prefetchRing(): void {
    const p = pos();
    const tt = t();
    const zz = z();
    const enabled = enabledChannels();
    const info = ds().info;
    const targets: PlaneAddr[] = [];
    for (const ch of enabled) {
      if (tt + 1 < info.n_time) targets.push({ p, t: tt + 1, c: ch.index, z: zz });
      if (tt >= 1) targets.push({ p, t: tt - 1, c: ch.index, z: zz });
      if (zz + 1 < info.n_z) targets.push({ p, t: tt, c: ch.index, z: zz + 1 });
      if (zz >= 1) targets.push({ p, t: tt, c: ch.index, z: zz - 1 });
    }
    for (const addr of targets) {
      if (cache.has(addr)) continue;
      // Fire-and-forget; populate cache.
      void fetchPlane(props.token, addr)
        .then((plane) => cache.set(addr, plane))
        .catch(() => {
          /* ignore prefetch failures */
        });
    }
  }

  // --- debounce render + prefetch on axis/channel changes ----------------
  createEffect(
    on(
      [pos, t, z, () => enabledChannels().map((c) => c.index).join(",")],
      () => {
        if (pendingTimer) clearTimeout(pendingTimer);
        pendingTimer = setTimeout(() => {
          void renderCurrent().then(() => prefetchRing());
        }, 50);
      },
      { defer: false },
    ),
  );

  // re-render when contrast/color changes (no refetch needed)
  createEffect(
    on(
      [
        () =>
          channels()
            .map((c) => `${c.index}:${c.visible}:${c.color}:${c.contrastMin}-${c.contrastMax}`)
            .join("|"),
      ],
      () => {
        // Re-composite from cached planes only; don't refetch.
        void recompositeFromCache();
      },
      { defer: true },
    ),
  );

  async function recompositeFromCache(): Promise<void> {
    const addrs = neededAddrs();
    if (addrs.length === 0) {
      drawComposite([]);
      return;
    }
    const planes: { channel: ChannelState; plane: Plane }[] = [];
    for (const addr of addrs) {
      const cached = cache.get(addr);
      if (!cached) {
        // Missing → fall back to full render (will fetch).
        void renderCurrent();
        return;
      }
      const ch = enabledChannels().find((c) => c.index === addr.c)!;
      planes.push({ channel: ch, plane: cached });
    }
    drawComposite(planes);
  }

  // --- apply transform (zoom/pan/fit) via CSS on the canvas ----------------
  createEffect(() => {
    if (!canvas) return;
    const z = fit() ? computeFit() : zoom();
    const p = pan();
    canvas.style.transformOrigin = "0 0";
    canvas.style.transform = `translate(${p.x}px, ${p.y}px) scale(${z})`;
  });

  function applyFit(): void {
    setFit(true);
    setZoom(1);
    setPan({ x: 0, y: 0 });
    if (container && canvas) {
      const z = computeFit();
      const d = ds();
      // center
      const c = container;
      const offX = (c.clientWidth - d.width * z) / 2;
      const offY = (c.clientHeight - d.height * z) / 2;
      setPan({ x: offX, y: offY });
    }
  }

  // Re-fit on resize.
  onMount(() => {
    const onResize = () => {
      if (fit()) applyFit();
    };
    window.addEventListener("resize", onResize);
    onCleanup(() => window.removeEventListener("resize", onResize));
    applyFit();
  });

  // --- zoom centered on cursor -------------------------------------------
  function zoomAt(factor: number, cx?: number, cy?: number): void {
    if (fit()) {
      // switch from fit to manual zoom; anchor at current center/cursor
      const curZ = computeFit();
      setFit(false);
      setZoom(curZ);
    }
    const c = container;
    if (!c || !canvas) return;
    const curZoom = zoom();
    const newZoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, curZoom * factor));
    if (newZoom === curZoom) return;
    // Anchor point: cursor (in container coords) or container center.
    const ax = cx ?? c.clientWidth / 2;
    const ay = cy ?? c.clientHeight / 2;
    const curPan = pan();
    // image coord under anchor: (ax - pan.x) / curZoom
    const imgX = (ax - curPan.x) / curZoom;
    const imgY = (ay - curPan.y) / curZoom;
    // new pan so that imgX, imgY stays under ax, ay at newZoom:
    const newPan = { x: ax - imgX * newZoom, y: ay - imgY * newZoom };
    setZoom(newZoom);
    setPan(newPan);
  }

  // --- keyboard -----------------------------------------------------------
  onMount(() => {
    const info = ds().info;
    const onKeyDown = (e: KeyboardEvent) => {
      // Don't intercept when typing in an input/select.
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "SELECT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      ) {
        return;
      }
      switch (e.key) {
        case "ArrowLeft":
          e.preventDefault();
          setT((v) => Math.max(0, v - 1));
          break;
        case "ArrowRight":
          e.preventDefault();
          setT((v) => Math.min(info.n_time - 1, v + 1));
          break;
        case "ArrowUp":
          e.preventDefault();
          setZ((v) => Math.max(0, v - 1));
          break;
        case "ArrowDown":
          e.preventDefault();
          setZ((v) => Math.min(info.n_z - 1, v + 1));
          break;
        case "a":
          e.preventDefault();
          autoContrastAll();
          break;
        case "f":
          e.preventDefault();
          applyFit();
          break;
        case "+":
        case "=":
          e.preventDefault();
          zoomAt(1.25);
          break;
        case "-":
        case "_":
          e.preventDefault();
          zoomAt(1 / 1.25);
          break;
        default:
          if (e.key >= "1" && e.key <= "9") {
            e.preventDefault();
            const n = Number(e.key) - 1;
            toggleChannel(n);
          }
          break;
      }
    };
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => window.removeEventListener("keydown", onKeyDown));
  });

  // --- channel UI actions -------------------------------------------------
  function toggleChannel(index: number): void {
    setChannels((cs) =>
      cs.map((c) => (c.index === index ? { ...c, visible: !c.visible } : c)),
    );
  }
  function setColor(index: number, color: string): void {
    setChannels((cs) =>
      cs.map((c) => (c.index === index ? { ...c, color } : c)),
    );
  }
  function setContrast(index: number, min: number, max: number): void {
    setChannels((cs) =>
      cs.map((c) => (c.index === index ? { ...c, contrastMin: min, contrastMax: max } : c)),
    );
  }

  function autoContrastAll(): void {
    const addrs = neededAddrs();
    const updates: Record<number, { min: number; max: number }> = {};
    for (const addr of addrs) {
      const cached = cache.get(addr);
      if (!cached) continue;
      const c = percentileContrast(cached);
      updates[addr.c] = c;
    }
    if (Object.keys(updates).length === 0) return;
    setChannels((cs) =>
      cs.map((c) =>
        updates[c.index]
          ? { ...c, contrastMin: updates[c.index].min, contrastMax: updates[c.index].max }
          : c,
      ),
    );
  }

  function autoContrastChannel(index: number): void {
    const addr: PlaneAddr = { p: pos(), t: t(), c: index, z: z() };
    const cached = cache.get(addr);
    if (!cached) return;
    const c = percentileContrast(cached);
    setContrast(index, c.min, c.max);
  }

  // --- position dropdown --------------------------------------------------
  function changePos(newPos: number): void {
    if (newPos === pos()) return;
    setPos(newPos);
    setT(0);
    setZ(0);
    cache.clear();
    applyFit();
  }

  // --- pixel inspector (mousemove) ---------------------------------------
  function onCanvasMouseMove(e: MouseEvent): void {
    if (!canvas || !container) return;
    const rect = canvas.getBoundingClientRect();
    // rect already reflects the CSS transform.
    const px = e.clientX - rect.left;
    const py = e.clientY - rect.top;
    const z = fit() ? computeFit() : zoom();
    const d = ds();
    const imgX = Math.floor(px / z);
    const imgY = Math.floor(py / z);
    if (imgX < 0 || imgY < 0 || imgX >= d.width || imgY >= d.height) {
      setPixel({ x: null, y: null, values: [] });
      return;
    }
    const addrs = neededAddrs();
    const values: { channel: ChannelState; value: number }[] = [];
    for (const addr of addrs) {
      const cached = cache.get(addr);
      if (!cached) continue;
      const ch = enabledChannels().find((c) => c.index === addr.c);
      if (!ch) continue;
      values.push({ channel: ch, value: cached[imgY * d.width + imgX] ?? 0 });
    }
    setPixel({ x: imgX, y: imgY, values });
  }
  function onCanvasMouseLeave(): void {
    setPixel({ x: null, y: null, values: [] });
  }

  // --- wheel zoom (optional but nice) ------------------------------------
  function onWheel(e: WheelEvent): void {
    if (!container) return;
    e.preventDefault();
    const rect = container.getBoundingClientRect();
    const cx = e.clientX - rect.left;
    const cy = e.clientY - rect.top;
    const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
    zoomAt(factor, cx, cy);
  }

  // --- drag-to-pan --------------------------------------------------------
  function onCanvasMouseDown(e: MouseEvent): void {
    if (!container) return;
    if (e.button !== 0) return;
    setFit(false);
    const cur = pan();
    dragging = { startX: e.clientX, startY: e.clientY, panX: cur.x, panY: cur.y };
    window.addEventListener("mousemove", onDragMove);
    window.addEventListener("mouseup", onDragUp);
  }
  function onDragMove(e: MouseEvent): void {
    if (!dragging) return;
    setPan({
      x: dragging!.panX + (e.clientX - dragging!.startX),
      y: dragging!.panY + (e.clientY - dragging!.startY),
    });
  }
  function onDragUp(): void {
    dragging = null;
    window.removeEventListener("mousemove", onDragMove);
    window.removeEventListener("mouseup", onDragUp);
  }

  // --- reload (fatal error recovery) -------------------------------------
  function reload(): void {
    setError(null);
    props.onFatal();
  }

  // --- render ------------------------------------------------------------
  return (
    <div class="flex-1 flex min-h-0">
      {/* sidebar */}
      <aside class="w-64 shrink-0 border-r border-neutral-800 overflow-y-auto p-3 space-y-4 bg-neutral-950">
        <Show when={ds().info.n_pos > 1}>
          <div>
            <label class="text-xs uppercase tracking-wide text-neutral-500 block mb-1">
              position
            </label>
            <select
              class="w-full bg-neutral-900 border border-neutral-800 rounded px-2 py-1 text-sm text-neutral-200"
              value={pos()}
              onChange={(e) => changePos(Number(e.currentTarget.value))}
            >
              <For each={Array.from({ length: ds().info.n_pos }, (_, i) => i)}>
                {(i) => <option value={i}>Pos {i}</option>}
              </For>
            </select>
          </div>
        </Show>

        <div class="grid grid-cols-3 gap-2 text-xs">
          <NavDim label="T" value={t()} max={ds().info.n_time} />
          <NavDim label="Z" value={z()} max={ds().info.n_z} />
          <NavDim label="P" value={pos()} max={ds().info.n_pos} />
        </div>

        <ChannelPanel
          channels={channels()}
          onToggle={toggleChannel}
          onColor={setColor}
          onContrast={setContrast}
          onAutoChannel={autoContrastChannel}
        />

        <MetadataPanel
          dataset={ds()}
          collapsed={metaCollapsed()}
          onToggle={() => setMetaCollapsed((v) => !v)}
        />
      </aside>

      {/* canvas area */}
      <div
        ref={container}
        class="relative flex-1 min-w-0 min-h-0 overflow-hidden bg-neutral-900"
        onWheel={onWheel}
      >
        <canvas
          ref={canvas}
          class="absolute top-0 left-0"
          style={{ "image-rendering": "pixelated" }}
          onMouseMove={onCanvasMouseMove}
          onMouseLeave={onCanvasMouseLeave}
          onMouseDown={onCanvasMouseDown}
        />
        {/* status bar */}
        <div class="absolute bottom-0 left-0 right-0 flex items-center gap-3 px-3 py-1.5 text-xs text-neutral-400 bg-black/50 border-t border-neutral-800">
          <span class="tabular-nums">
            T {t() + 1}/{ds().info.n_time}
          </span>
          <span class="tabular-nums">
            Z {z() + 1}/{ds().info.n_z}
          </span>
          <Show when={ds().info.n_pos > 1}>
            <span class="tabular-nums">
              Pos {pos() + 1}/{ds().info.n_pos}
            </span>
          </Show>
          <span class="tabular-nums">
            {fit() ? "fit" : `${zoom().toFixed(2)}x`}
          </span>
          <Show when={rendering()}>
            <span class="text-amber-400">rendering…</span>
          </Show>
          <div class="ml-auto flex gap-2 text-neutral-500">
            <span>←→ T  ↑↓ Z  1-9 ch  a auto  f fit  +− zoom</span>
          </div>
        </div>

        {/* pixel inspector */}
        <div class="absolute top-2 right-2 w-44">
          <PixelInspector
            x={pixel().x}
            y={pixel().y}
            values={pixel().values}
          />
        </div>

        <ErrorOverlay error={error()} onReload={reload} />
      </div>
    </div>
  );
};

const NavDim: Component<{ label: string; value: number; max: number }> = (
  props,
) => (
  <div class="border border-neutral-800 rounded px-2 py-1">
    <div class="text-neutral-500 text-[10px] uppercase">{props.label}</div>
    <div class="text-neutral-200 tabular-nums">
      {props.value + 1}/{props.max}
    </div>
  </div>
);

export default Viewer;