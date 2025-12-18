import { Show, onMount } from "solid-js";
import { emit } from "@tauri-apps/api/event";
import type { Item, ActionInfo } from "./core/types";
import * as tauri from "./core/tauri";
import {
  CommandList,
  CommandInput,
  CommandItems,
  CommandEmpty,
} from "~/components/command";
import {
  GroupedResults,
  buildItemMap,
  getSelectedItems,
  getAllItemIds,
} from "~/components/search";
import { ViewStack } from "~/components/ViewStack";
import {
  ActionMenu,
  ActionProgress,
  ActionComplete,
  ActionFailed,
} from "~/components/ActionMenu";
import { useLauncher } from "~/store";
import { useTauriEvents, useGlobalKeyboard, usePanelLifecycle } from "~/shared/hooks";
import { useSearch } from "~/features/search";
import { useActions } from "~/features/actions";
import "./App.css";

function App() {
  const [state, actions] = useLauncher();

  // Setup Tauri event listeners (direct store updates)
  useTauriEvents();

  // Setup debounced search effect (triggers on query change)
  useSearch();

  // Get action handlers
  const { runDefaultAction, showActionsMenu, executeSelectedAction } = useActions();

  // Refs
  let inputRef: HTMLInputElement | undefined;

  // Build item lookup map
  const itemsMap = () => buildItemMap(state.search.groups);

  // Get items for action execution based on cursor or selection
  const getItemsForAction = (cursorId: string): Item[] => {
    const selected = getSelectedItems(state.search.groups, state.selection.ids);
    if (selected.length > 0) {
      return selected;
    }
    // If nothing selected, use the cursor item
    const cursorItem = itemsMap().get(cursorId);
    return cursorItem ? [cursorItem] : [];
  };

  // Handle activation (Enter key)
  const handleActivate = async (id: string) => {
    const items = getItemsForAction(id);
    if (items.length === 0) return;

    const result = await runDefaultAction(items);

    // Handle dismiss result (useActions handles feedback)
    if (result.type === "Dismiss") {
      resetAndHide();
    }
  };

  // Handle escape key
  const handleEscape = async () => {
    if (state.ui.actionMenu.visible) {
      actions.hideActionMenu();
      return;
    }

    // Try to pop the view stack first
    const popped = await tauri.popView();
    if (!popped) {
      // At root view, dismiss
      resetAndHide();
    }
  };

  // Reset state and hide panel
  const resetAndHide = async () => {
    actions.reset();
    await tauri.dismiss();
  };

  // Show action menu (Tab key handler)
  const handleShowActions = async (cursorId: string) => {
    const items = getItemsForAction(cursorId);
    if (items.length === 0) return;

    await showActionsMenu(items);
  };

  // Execute selected action from menu
  const handleActionSelect = async (action: ActionInfo) => {
    const items = getSelectedItems(state.search.groups, state.selection.ids);
    const itemsToUse = items.length > 0 ? items : (() => {
      // Use cursor item - for now, use first item in the groups
      const allItems = getAllItemIds(state.search.groups);
      if (allItems.length > 0) {
        const firstItem = itemsMap().get(allItems[0]);
        return firstItem ? [firstItem] : [];
      }
      return [];
    })();

    if (itemsToUse.length === 0) return;

    const result = await executeSelectedAction(action, itemsToUse);

    // Handle dismiss result
    if (result.type === "Dismiss") {
      resetAndHide();
    }
  };

  // Navigate view stack via breadcrumb click
  const handleViewStackNavigate = async (index: number) => {
    await tauri.popToView(index);
  };

  // Setup global keyboard shortcuts
  useGlobalKeyboard({
    Tab: (e) => {
      if (!e.shiftKey) {
        e.preventDefault();
        const allIds = getAllItemIds(state.search.groups);
        if (allIds.length > 0) {
          handleShowActions(allIds[0]);
        }
      }
    },
  });

  // Setup panel lifecycle (focus input and trigger initial search on show)
  usePanelLifecycle({
    inputRef: () => inputRef,
    onSearchResults: actions.setGroups,
  });

  // Signal backend that frontend is ready - after all hooks have registered their listeners
  onMount(async () => {
    // Flush microtask queue to ensure all async listener registrations complete
    await Promise.resolve();
    console.log("[lux] frontend ready");
    await emit('lux:frontend-ready');
  });

  return (
    <div class="h-full flex flex-col relative">
      <CommandList
        class="h-full flex flex-col border rounded-xl"
        style="border-color: rgba(255, 255, 255, 0.15)"
        onActivate={handleActivate}
        onEscape={handleEscape}
        selectionMode={state.selection.mode}
        selectedIds={state.selection.ids}
        items={actions.flatItems}
        cursorIndex={() => state.selection.cursorIndex}
        onCursorChange={actions.setCursorIndex}
      >
        {/* View stack breadcrumbs */}
        <ViewStack stack={state.ui.viewStack} onNavigate={handleViewStackNavigate} />

        {/* Search input */}
        <CommandInput
          ref={inputRef}
          placeholder={state.ui.placeholder}
          value={state.search.query}
          onValueChange={actions.setQuery}
          autofocus
        />

        {/* Results area */}
        <CommandItems class="flex-1 p-2">
          <CommandEmpty class="text-muted-foreground">
            No results found.
          </CommandEmpty>
          <GroupedResults
            groups={state.search.groups}
            selectionMode={state.selection.mode}
            selectedIds={() => state.selection.ids}
          />
        </CommandItems>

        {/* Action feedback */}
        <Show when={state.feedback.progress}>
          <ActionProgress
            message={state.feedback.progress!}
            onCancel={() => actions.setProgress(null)}
          />
        </Show>

        <Show when={state.feedback.complete}>
          <ActionComplete
            message={state.feedback.complete!}
            onDismiss={() => actions.setComplete(null)}
          />
        </Show>

        <Show when={state.feedback.error}>
          <ActionFailed
            error={state.feedback.error!}
            onDismiss={() => actions.setError(null)}
          />
        </Show>
      </CommandList>

      {/* Action menu overlay */}
      <ActionMenu
        actions={state.ui.actionMenu.actions}
        open={state.ui.actionMenu.visible}
        onSelect={handleActionSelect}
        onClose={actions.hideActionMenu}
      />
    </div>
  );
}

export default App;
