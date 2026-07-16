import { createMemo, Show, type Component } from "solid-js";
import type { Dataset } from "../types";

/** Collapsible metadata panel: dims, channels, normalized + raw metadata. */
const MetadataPanel: Component<{
  dataset: Dataset;
  collapsed: boolean;
  onToggle: () => void;
}> = (props) => {
  const normalizedJson = createMemo(() => {
    try {
      return JSON.stringify(props.dataset.metadata.normalized, null, 2);
    } catch {
      return String(props.dataset.metadata.normalized);
    }
  });
  return (
    <div class="space-y-2">
      <button
        type="button"
        class="text-xs uppercase tracking-wide text-neutral-500 hover:text-neutral-300 w-full text-left"
        onClick={() => props.onToggle()}
      >
        {props.collapsed ? "▸ metadata" : "▾ metadata"}
      </button>
      <Show when={!props.collapsed}>
        <div class="text-xs space-y-2">
          <div class="grid grid-cols-2 gap-x-2 gap-y-0.5">
            <Field label="positions" value={String(props.dataset.info.n_pos)} />
            <Field label="timepoints" value={String(props.dataset.info.n_time)} />
            <Field label="channels" value={String(props.dataset.info.n_chan)} />
            <Field label="z-slices" value={String(props.dataset.info.n_z)} />
            <Field
              label="width"
              value={String(props.dataset.width)}
            />
            <Field
              label="height"
              value={String(props.dataset.height)}
            />
          </div>
          <div>
            <div class="text-neutral-500 mb-0.5">channels</div>
            <ul class="space-y-0.5">
              {props.dataset.channels.map((c) => (
                <li class="flex items-center gap-1.5">
                  <span
                    class="inline-block w-2.5 h-2.5 rounded-full border border-neutral-700"
                    style={{ "background-color": c.color }}
                  />
                  <span class="text-neutral-300">{c.name}</span>
                  <span class="text-neutral-600">#{c.index}</span>
                </li>
              ))}
            </ul>
          </div>
          <div>
            <div class="text-neutral-500 mb-0.5">normalized</div>
            <pre class="text-[10px] leading-tight text-neutral-300 bg-neutral-950 border border-neutral-800 rounded p-2 overflow-x-auto max-h-60">
              {normalizedJson()}
            </pre>
          </div>
          <Show when={props.dataset.metadata.raw}>
            <div>
              <div class="text-neutral-500 mb-0.5">
                raw {props.dataset.metadata.raw_format
                  ? `(${props.dataset.metadata.raw_format})`
                  : ""}
              </div>
              <pre class="text-[10px] leading-tight text-neutral-300 bg-neutral-950 border border-neutral-800 rounded p-2 overflow-x-auto max-h-40">
                {props.dataset.metadata.raw}
              </pre>
            </div>
          </Show>
        </div>
      </Show>
    </div>
  );
};

const Field: Component<{ label: string; value: string }> = (props) => (
  <div>
    <span class="text-neutral-500">{props.label}: </span>
    <span class="text-neutral-200 tabular-nums">{props.value}</span>
  </div>
);

export default MetadataPanel;