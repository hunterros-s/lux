import { For, Show, createSignal } from "solid-js";
import type { ActionInfo } from "~/core/types";

export interface ActionMenuProps {
  /** List of available actions */
  actions: ActionInfo[];
  /** Called when an action is selected */
  onSelect: (action: ActionInfo) => void;
  /** Called when the menu should close */
  onClose: () => void;
  /** Whether the menu is visible */
  open: boolean;
}

/**
 * Displays a menu of available actions for the selected item(s).
 * Triggered by Tab key or action button.
 */
export function ActionMenu(props: ActionMenuProps) {
  const [selectedIndex, setSelectedIndex] = createSignal(0);

  const handleKeyDown = (e: KeyboardEvent) => {
    if (!props.open) return;

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        setSelectedIndex((prev) =>
          prev < props.actions.length - 1 ? prev + 1 : 0
        );
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        setSelectedIndex((prev) =>
          prev > 0 ? prev - 1 : props.actions.length - 1
        );
        break;
      }
      case "Enter": {
        e.preventDefault();
        const action = props.actions[selectedIndex()];
        if (action) {
          props.onSelect(action);
        }
        break;
      }
      case "Escape": {
        e.preventDefault();
        props.onClose();
        break;
      }
    }
  };

  return (
    <Show when={props.open && props.actions.length > 0}>
      <div
        class="absolute bottom-full left-0 right-0 mb-2 mx-4 bg-popover border rounded-lg shadow-lg overflow-hidden"
        style="border-color: rgba(255, 255, 255, 0.1)"
        onKeyDown={handleKeyDown}
        tabindex="-1"
      >
        <div class="p-1">
          <For each={props.actions}>
            {(action, index) => (
              <button
                class={`w-full flex items-center gap-3 px-3 py-2 rounded-md text-sm transition-colors ${
                  index() === selectedIndex()
                    ? "bg-accent text-accent-foreground"
                    : "hover:bg-accent/50"
                }`}
                onClick={() => props.onSelect(action)}
                onMouseEnter={() => setSelectedIndex(index())}
              >
                <Show when={action.icon}>
                  <span class="text-lg">{action.icon}</span>
                </Show>
                <span class="flex-1 text-left">{action.title}</span>
                <Show when={index() === 0}>
                  <span class="text-xs text-muted-foreground">default</span>
                </Show>
              </button>
            )}
          </For>
        </div>
      </div>
    </Show>
  );
}

// =============================================================================
// Action Progress/Complete/Failed UI components
// =============================================================================

export interface ActionProgressProps {
  message: string;
  onCancel?: () => void;
}

/**
 * Shows progress indicator during action execution.
 */
export function ActionProgress(props: ActionProgressProps) {
  return (
    <div class="flex items-center gap-3 px-4 py-3 bg-secondary/50 border-t" style="border-color: rgba(255, 255, 255, 0.1)">
      <div class="size-4 border-2 border-primary border-t-transparent rounded-full animate-spin" />
      <span class="flex-1 text-sm">{props.message}</span>
      <Show when={props.onCancel}>
        <button
          class="text-sm text-muted-foreground hover:text-foreground"
          onClick={props.onCancel}
        >
          Cancel
        </button>
      </Show>
    </div>
  );
}

export interface ActionCompleteProps {
  message: string;
  onDismiss: () => void;
}

/**
 * Shows completion message after action succeeds.
 */
export function ActionComplete(props: ActionCompleteProps) {
  return (
    <div class="flex items-center gap-3 px-4 py-3 bg-green-500/10 border-t" style="border-color: rgba(34, 197, 94, 0.2)">
      <svg class="size-4 text-green-500" viewBox="0 0 16 16" fill="currentColor">
        <path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm3.78 5.28-4.5 6a.75.75 0 0 1-1.18.02l-2-2.5a.75.75 0 1 1 1.18-.92l1.36 1.7 3.96-5.28a.75.75 0 0 1 1.18.98z" />
      </svg>
      <span class="flex-1 text-sm text-green-400">{props.message}</span>
      <button
        class="text-sm text-muted-foreground hover:text-foreground"
        onClick={props.onDismiss}
      >
        Dismiss
      </button>
    </div>
  );
}

export interface ActionFailedProps {
  error: string;
  onDismiss: () => void;
}

/**
 * Shows error message when action fails.
 */
export function ActionFailed(props: ActionFailedProps) {
  return (
    <div class="flex items-center gap-3 px-4 py-3 bg-red-500/10 border-t" style="border-color: rgba(239, 68, 68, 0.2)">
      <svg class="size-4 text-red-500" viewBox="0 0 16 16" fill="currentColor">
        <path d="M8 0a8 8 0 1 0 0 16A8 8 0 0 0 8 0zm3.5 10.44a.75.75 0 0 1-1.06 1.06L8 9.06l-2.44 2.44a.75.75 0 0 1-1.06-1.06L6.94 8 4.5 5.56a.75.75 0 0 1 1.06-1.06L8 6.94l2.44-2.44a.75.75 0 0 1 1.06 1.06L9.06 8l2.44 2.44z" />
      </svg>
      <span class="flex-1 text-sm text-red-400">{props.error}</span>
      <button
        class="text-sm text-muted-foreground hover:text-foreground"
        onClick={props.onDismiss}
      >
        Dismiss
      </button>
    </div>
  );
}
