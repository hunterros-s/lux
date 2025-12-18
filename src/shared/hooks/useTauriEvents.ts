/**
 * Tauri event bridge hook.
 *
 * Bridges Tauri events directly to store actions without intermediate event bus.
 * Properly handles async listener setup with synchronous cleanup.
 */
import { onMount, onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { useLauncher } from "~/store";
import type { Groups, ViewState } from "~/core/types";
import { handleProgress, handleComplete, handleError } from "~/lib/actionFeedback";

/**
 * Setup Tauri event listeners that directly update the store.
 *
 * This hook:
 * - Registers listeners on mount
 * - Updates store directly (no intermediate event bus)
 * - Cleans up listeners synchronously on unmount
 *
 * @example
 * ```tsx
 * function App() {
 *   useTauriEvents();
 *   // ...
 * }
 * ```
 */
export function useTauriEvents() {
  const [, actions] = useLauncher();

  // Store unlisten functions for synchronous cleanup
  const unlisteners: (() => void)[] = [];

  onMount(async () => {
    // Register all event listeners and collect unlisten functions
    const listeners = await Promise.all([
      // Panel visibility
      listen('lux:panel-shown', () => {
        // Panel shown is handled by App.tsx for focus management
        // Store doesn't track panel visibility
      }),

      listen('lux:panel-hidden', () => {
        actions.reset();
      }),

      // Search results
      listen<Groups>('lux:results-updated', (event) => {
        actions.setGroups(event.payload);
      }),

      // View state
      listen<ViewState>('lux:view-state-changed', (event) => {
        const viewState = event.payload;
        actions.setSelectionMode(viewState.selectionMode);
        actions.setSelectedIds(new Set(viewState.selectedIds));
        if (viewState.placeholder) {
          actions.setPlaceholder(viewState.placeholder);
        }
        // Sync query from backend if it changed
        if (viewState.query !== undefined) {
          actions.setQuery(viewState.query);
        }
      }),

      // Action feedback
      listen<string>('lux:action-progress', (event) => {
        handleProgress(actions, event.payload);
      }),

      listen<string>('lux:action-complete', (event) => {
        handleComplete(actions, event.payload);
      }),

      listen<string>('lux:action-failed', (event) => {
        handleError(actions, event.payload);
      }),

      // Plugin errors (log only, don't show to user)
      listen<string>('lux:plugin-error', (event) => {
        console.error('Plugin error:', event.payload);
      }),
    ]);

    // Store all unlisten functions
    unlisteners.push(...listeners);
  });

  // Synchronous cleanup - all unlisteners are already resolved functions
  onCleanup(() => {
    unlisteners.forEach(fn => fn());
  });
}
