import { For, Show } from "solid-js";
import type { ViewStackEntry } from "~/core/types";

export interface ViewStackProps {
  /** Array of views in the stack */
  stack: ViewStackEntry[];
  /** Called when user clicks a breadcrumb to navigate */
  onNavigate: (index: number) => void;
}

/**
 * Renders breadcrumb navigation for the view stack.
 * Only visible when there's more than one view in the stack.
 */
export function ViewStack(props: ViewStackProps) {
  return (
    <Show when={props.stack.length > 1}>
      <div class="flex items-center gap-1 px-4 py-2 text-sm border-b" style="border-color: rgba(255, 255, 255, 0.1)">
        <For each={props.stack}>
          {(view, index) => (
            <>
              <button
                class={`px-1.5 py-0.5 rounded hover:bg-secondary transition-colors ${
                  index() === props.stack.length - 1
                    ? "text-foreground font-medium"
                    : "text-muted-foreground hover:text-foreground"
                }`}
                onClick={() => props.onNavigate(view.index)}
                disabled={index() === props.stack.length - 1}
              >
                {view.title || "Search"}
              </button>
              <Show when={index() < props.stack.length - 1}>
                <span class="text-muted-foreground/50">›</span>
              </Show>
            </>
          )}
        </For>
      </div>
    </Show>
  );
}
