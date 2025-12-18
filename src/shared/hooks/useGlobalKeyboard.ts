/**
 * Global keyboard event hook.
 *
 * Manages document-level keyboard event listeners with proper cleanup.
 */
import { onMount, onCleanup } from "solid-js";

export type KeyboardHandlers = {
  [key: string]: (e: KeyboardEvent) => void;
};

/**
 * Setup global keyboard event listeners.
 *
 * @param handlers - Map of key names to handler functions
 *
 * @example
 * ```tsx
 * useGlobalKeyboard({
 *   Tab: (e) => {
 *     if (!e.shiftKey) {
 *       e.preventDefault();
 *       showActionsMenu();
 *     }
 *   },
 * });
 * ```
 */
export function useGlobalKeyboard(handlers: KeyboardHandlers): void {
  const handleKeyDown = (e: KeyboardEvent) => {
    const handler = handlers[e.key];
    if (handler) {
      handler(e);
    }
  };

  onMount(() => {
    document.addEventListener("keydown", handleKeyDown);

    onCleanup(() => {
      document.removeEventListener("keydown", handleKeyDown);
    });
  });
}
