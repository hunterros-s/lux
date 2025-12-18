import { Show } from "solid-js";

export interface ItemIconProps {
  icon?: string | null;
  fallback?: string;
  class?: string;
}

/**
 * Renders an item icon with support for:
 * - Base64 data URLs (data:image/png;base64,...)
 * - Text/emoji fallback
 */
export function ItemIcon(props: ItemIconProps) {
  const isDataUrl = () => props.icon?.startsWith("data:image/");
  const fallback = () => props.fallback ?? "•";

  return (
    <div
      class={`size-[var(--size-item-icon)] rounded-lg shrink-0 flex items-center justify-center overflow-hidden ${props.class ?? ""}`}
    >
      <Show
        when={isDataUrl()}
        fallback={
          <span class="text-muted-foreground bg-secondary size-full flex items-center justify-center rounded-lg">
            {props.icon || fallback()}
          </span>
        }
      >
        <img
          src={props.icon!}
          alt=""
          class="size-full object-contain"
          draggable={false}
        />
      </Show>
    </div>
  );
}
