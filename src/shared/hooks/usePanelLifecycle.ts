/**
 * Panel lifecycle hook.
 *
 * Manages panel visibility events and displays search results from event payload.
 * Separated from useTauriEvents because it needs access to DOM refs.
 */
import { onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import type { Groups, PanelShownPayload } from "~/core/types";

export type PanelLifecycleOptions = {
  /** Ref to the input element to focus on panel show */
  inputRef: () => HTMLInputElement | undefined;
  /** Callback to update search results */
  onSearchResults: (groups: Groups) => void;
};

/**
 * Setup panel lifecycle event listeners.
 *
 * Handles:
 * - Focusing input when panel is shown
 * - Displaying search results from event payload
 * - Signaling backend when frontend is ready
 *
 * @example
 * ```tsx
 * let inputRef: HTMLInputElement | undefined;
 *
 * usePanelLifecycle({
 *   inputRef: () => inputRef,
 *   onSearchResults: actions.setGroups,
 * });
 * ```
 */
export function usePanelLifecycle(options: PanelLifecycleOptions): void {
  const { inputRef, onSearchResults } = options;

  onMount(async () => {
    const unlistenPanelShown = await listen<PanelShownPayload>(
      'lux:panel-shown',
      (event) => {
        console.log("[lux] panel-shown:", event.payload.results?.length ?? 0, "groups");
        inputRef()?.focus();

        if (event.payload.results) {
          onSearchResults(event.payload.results);
        } else {
          console.error("[lux] panel-shown missing results");
        }
      }
    );

    // Note: frontend-ready emission moved to App.tsx for proper coordination
    // with other hooks' async listener registration

    onCleanup(() => {
      unlistenPanelShown();
    });
  });
}
