import { For, type Accessor } from "solid-js";
import type { Groups, Item, SelectionMode } from "~/core/types";
import { CommandGroup } from "~/components/command";
import { SearchResultItem } from "./SearchResultItem";

export interface GroupedResultsProps {
  /** Groups of items to render */
  groups: Groups;
  /** Selection mode for the current view */
  selectionMode: SelectionMode;
  /** Set of selected item IDs */
  selectedIds: Accessor<Set<string>>;
}

/**
 * Renders grouped search results with optional group headers and selection support.
 */
export function GroupedResults(props: GroupedResultsProps) {
  const showCheckbox = () => props.selectionMode === 'multi';

  const isSelected = (id: string) => props.selectedIds().has(id);

  return (
    <For each={props.groups}>
      {(group) => (
        <CommandGroup heading={group.title}>
          <For each={group.items}>
            {(item) => (
              <SearchResultItem
                item={item}
                isSelected={isSelected(item.id)}
                showCheckbox={showCheckbox()}
              />
            )}
          </For>
        </CommandGroup>
      )}
    </For>
  );
}

/**
 * Get all item IDs from groups for selection operations.
 */
export function getAllItemIds(groups: Groups): string[] {
  return groups.flatMap(group => group.items.map(item => item.id));
}

/**
 * Build a lookup map from item ID to Item for quick access.
 */
export function buildItemMap(groups: Groups): Map<string, Item> {
  const map = new Map<string, Item>();
  for (const group of groups) {
    for (const item of group.items) {
      map.set(item.id, item);
    }
  }
  return map;
}

/**
 * Get selected items from groups.
 */
export function getSelectedItems(groups: Groups, selectedIds: Set<string>): Item[] {
  const items: Item[] = [];
  for (const group of groups) {
    for (const item of group.items) {
      if (selectedIds.has(item.id)) {
        items.push(item);
      }
    }
  }
  return items;
}
