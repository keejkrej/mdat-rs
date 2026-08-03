import { For, Show, type Component } from "solid-js";
import type { ChannelState } from "../types";

/** Pixel inspector overlay: shows raw u16 values per enabled channel at the
 * hovered pixel, plus (x,y). */
const PixelInspector: Component<{
  x: number | null;
  y: number | null;
  values: { channel: ChannelState; value: number }[];
}> = (props) => {
  return (
    <div class="border border-neutral-800 rounded p-2 text-xs space-y-1">
      <div class="text-neutral-500 uppercase tracking-wide">pixel</div>
      <Show
        when={props.x !== null && props.y !== null}
        fallback={<div class="text-neutral-600">hover the canvas</div>}
      >
        <div class="text-neutral-300 tabular-nums">
          x={props.x} y={props.y}
        </div>
        <Show when={props.values.length === 0}>
          <div class="text-neutral-600">no enabled channels</div>
        </Show>
        <For each={props.values}>
          {(v) => (
            <div class="flex items-center gap-1.5">
              <span
                class="inline-block w-2.5 h-2.5 rounded-full border border-neutral-700"
                style={{ "background-color": v.channel.color }}
              />
              <span class="text-neutral-300 truncate flex-1">
                {v.channel.name}
              </span>
              <span class="text-neutral-100 tabular-nums">{v.value}</span>
            </div>
          )}
        </For>
      </Show>
    </div>
  );
};

export default PixelInspector;