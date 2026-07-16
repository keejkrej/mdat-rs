import { createSignal, onMount, Show, type Component } from "solid-js";
import { tokenFromPath, fetchDataset } from "./api";
import type { Dataset } from "./types";
import Viewer from "./Viewer";

const App: Component = () => {
  const [dataset, setDataset] = createSignal<Dataset | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [loading, setLoading] = createSignal(true);
  const [token, setToken] = createSignal<string | null>(null);

  async function load(tok: string): Promise<void> {
    setLoading(true);
    setError(null);
    try {
      const ds = await fetchDataset(tok);
      setDataset(ds);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(
        `failed to load dataset: ${msg}\n\n(the server may have exited — check the terminal)`,
      );
    } finally {
      setLoading(false);
    }
  }

  onMount(async () => {
    const tok = tokenFromPath();
    setToken(tok);
    if (!tok) {
      setError("no token in URL — open the URL printed by `mdat view`");
      setLoading(false);
      return;
    }
    await load(tok);
  });

  function reload(): void {
    if (token()) void load(token()!);
  }

  return (
    <div class="h-full flex flex-col">
      <header class="border-b border-neutral-800 px-4 py-2 flex items-baseline gap-3 shrink-0">
        <h1 class="text-sm font-semibold tracking-tight text-neutral-100">
          {dataset()?.name ?? "mdat view"}
        </h1>
        <Show when={token()}>
          <code class="text-xs text-neutral-600 truncate">{token()}</code>
        </Show>
        <Show when={dataset()}>
          <span class="text-xs text-neutral-500">
            {dataset()!.width}×{dataset()!.height}px · {dataset()!.info.n_chan}ch
          </span>
        </Show>
      </header>

      <Show when={loading()}>
        <div class="flex-1 flex items-center justify-center text-neutral-400">
          loading dataset…
        </div>
      </Show>

      <Show when={error() && !dataset()}>
        <div class="flex-1 flex items-center justify-center p-4">
          <div class="max-w-md border border-red-800 bg-red-950/40 text-red-200 rounded p-4 whitespace-pre-wrap">
            {error()}
            <Show when={token()}>
              <div class="mt-3">
                <button
                  type="button"
                  class="px-3 py-1.5 text-sm rounded bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 text-neutral-100"
                  onClick={reload}
                >
                  Reload
                </button>
              </div>
            </Show>
          </div>
        </div>
      </Show>

      <Show when={dataset() && !loading()}>
        <Viewer
          dataset={dataset()!}
          token={token()!}
          onFatal={reload}
        />
      </Show>
    </div>
  );
};

export default App;