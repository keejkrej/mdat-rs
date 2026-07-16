import type { Component } from "solid-js";
import type { ViewerError } from "../types";

const ErrorOverlay: Component<{
  error: ViewerError | null;
  onReload: () => void;
}> = (props) => {
  if (!props.error) return null;
  return (
    <div class="absolute inset-0 z-50 flex items-center justify-center bg-black/70 backdrop-blur-sm">
      <div class="max-w-md w-full mx-4 border border-red-800 bg-neutral-900 rounded-lg p-5 shadow-xl">
        <h3 class="text-red-300 font-semibold text-sm uppercase tracking-wide mb-2">
          {props.error.fatal ? "connection lost" : "error"}
        </h3>
        <p class="text-neutral-200 text-sm whitespace-pre-wrap break-words">
          {props.error.message}
        </p>
        {props.error.frame ? (
          <p class="mt-2 text-xs text-neutral-400">
            frame: p={props.error.frame.p} t={props.error.frame.t} c={props.error.frame.c} z={props.error.frame.z}
          </p>
        ) : null}
        {props.error.fatal ? (
          <>
            <p class="mt-3 text-xs text-neutral-400">
              the server may have exited — check the terminal
            </p>
            <div class="mt-4 flex gap-2">
              <button
                type="button"
                class="px-3 py-1.5 text-sm rounded bg-neutral-800 hover:bg-neutral-700 border border-neutral-700 text-neutral-100"
                onClick={() => props.onReload()}
              >
                Reload
              </button>
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
};

export default ErrorOverlay;