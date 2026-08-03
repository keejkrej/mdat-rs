import { For, type Component } from "solid-js";
import type { ChannelState } from "../types";

/** Per-channel panel: visibility toggle, color swatch + color input, contrast
 * min/max number inputs, and auto-contrast per channel. */
const ChannelPanel: Component<{
  channels: ChannelState[];
  onToggle: (index: number) => void;
  onColor: (index: number, color: string) => void;
  onContrast: (index: number, min: number, max: number) => void;
  onAutoChannel: (index: number) => void;
}> = (props) => {
  return (
    <div class="space-y-2">
      <h2 class="text-xs uppercase tracking-wide text-neutral-500">channels</h2>
      <For each={props.channels}>
        {(ch) => (
          <div
            class="border border-neutral-800 rounded p-2 text-sm"
            classList={{
              "opacity-40": !ch.visible,
              "bg-neutral-900": ch.visible,
            }}
          >
            <div class="flex items-center gap-2">
              <button
                type="button"
                class="w-5 h-5 rounded border border-neutral-700 flex items-center justify-center text-xs"
                classList={{ "bg-neutral-700 text-white": ch.visible }}
                title={`toggle channel ${ch.index + 1}`}
                onClick={() => props.onToggle(ch.index)}
              >
                {ch.visible ? "●" : "○"}
              </button>
              <input
                type="color"
                value={ch.color}
                class="w-5 h-5 bg-transparent border-0 cursor-pointer p-0"
                onInput={(e) => props.onColor(ch.index, e.currentTarget.value)}
              />
              <span class="text-neutral-200 truncate flex-1">{ch.name}</span>
              <span class="text-neutral-600 text-xs">#{ch.index}</span>
            </div>
            <div class="flex items-center gap-2 mt-1.5">
              <label class="text-xs text-neutral-500 w-6">min</label>
              <input
                type="number"
                min="0"
                max="65535"
                value={ch.contrastMin}
                class="w-20 bg-neutral-950 border border-neutral-800 rounded px-1 py-0.5 text-xs text-neutral-200"
                onChange={(e) =>
                  props.onContrast(
                    ch.index,
                    Number(e.currentTarget.value),
                    ch.contrastMax,
                  )
                }
              />
              <label class="text-xs text-neutral-500 w-6">max</label>
              <input
                type="number"
                min="0"
                max="65535"
                value={ch.contrastMax}
                class="w-20 bg-neutral-950 border border-neutral-800 rounded px-1 py-0.5 text-xs text-neutral-200"
                onChange={(e) =>
                  props.onContrast(
                    ch.index,
                    ch.contrastMin,
                    Number(e.currentTarget.value),
                  )
                }
              />
              <button
                type="button"
                class="text-xs text-neutral-400 hover:text-neutral-100 ml-auto"
                title="auto-contrast this channel (current frame)"
                onClick={() => props.onAutoChannel(ch.index)}
              >
                auto
              </button>
            </div>
          </div>
        )}
      </For>
    </div>
  );
};

export default ChannelPanel;