import { Show } from "solid-js";
import type { Item } from "~/core/types";
import { CommandItem } from "~/components/command";
import { ItemIcon } from "./ItemIcon";

export interface SearchResultItemProps {
  item: Item;
  /** Whether this item is selected (multi-select mode) */
  isSelected?: boolean;
  /** Whether to show selection checkbox (multi-select mode) */
  showCheckbox?: boolean;
}

/**
 * A single search result row with icon, title, subtitle, and optional selection indicator.
 */
export function SearchResultItem(props: SearchResultItemProps) {
  return (
    <CommandItem
      id={props.item.id}
      class="flex items-center gap-[var(--spacing-item-gap)] px-[var(--spacing-item-x)] py-[var(--spacing-item-y)] rounded-lg cursor-pointer"
    >
      {/* Selection checkbox for multi-select mode */}
      <Show when={props.showCheckbox}>
        <div class="size-4 shrink-0 flex items-center justify-center">
          <Show
            when={props.isSelected}
            fallback={
              <div class="size-4 rounded border border-muted-foreground/40" />
            }
          >
            <div class="size-4 rounded bg-primary flex items-center justify-center">
              <svg
                class="size-3 text-primary-foreground"
                viewBox="0 0 12 12"
                fill="none"
                stroke="currentColor"
                stroke-width="2"
              >
                <path d="M2 6l3 3 5-6" />
              </svg>
            </div>
          </Show>
        </div>
      </Show>

      <ItemIcon icon={props.item.icon} />
      <div class="flex items-center gap-2 min-w-0 flex-1">
        <span class="font-medium text-[length:var(--text-item-title)] leading-none truncate">
          {props.item.title}
        </span>
        <Show when={props.item.subtitle}>
          <span class="text-[length:var(--text-item-subtitle)] leading-none text-muted-foreground truncate">
            {props.item.subtitle}
          </span>
        </Show>
      </div>
    </CommandItem>
  );
}
