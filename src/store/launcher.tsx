/**
 * Centralized Solid.js store for Lux launcher state management.
 *
 * State is split by update frequency to minimize re-renders:
 * - search: Changes on every keystroke
 * - selection: Changes on navigation
 * - ui: Changes rarely (view stack, action menu)
 * - feedback: Transient feedback messages
 */

import { createContext, useContext, batch, createEffect, type ParentComponent } from "solid-js";
import { createStore, reconcile } from "solid-js/store";
import type { Groups, ActionInfo, SelectionMode, ViewStackEntry } from "../core/types";

/**
 * Flatten groups into a list of item IDs in display order.
 */
function flattenGroups(groups: Groups): string[] {
  if (!groups || !Array.isArray(groups)) return [];
  const items: string[] = [];
  for (const group of groups) {
    for (const item of group.items) {
      items.push(item.id);
    }
  }
  return items;
}

// =============================================================================
// State Types
// =============================================================================

/**
 * State split by update frequency to minimize re-renders.
 */
export type LauncherState = {
  /** Changes on every keystroke */
  search: {
    query: string;
    groups: Groups;
    loading: boolean;
  };
  /** Changes on navigation */
  selection: {
    ids: Set<string>;
    mode: SelectionMode;
    cursorIndex: number;
  };
  /** Changes rarely */
  ui: {
    placeholder: string;
    viewStack: ViewStackEntry[];
    actionMenu: {
      visible: boolean;
      actions: ActionInfo[];
    };
  };
  /** Transient feedback */
  feedback: {
    progress: string | null;
    complete: string | null;
    error: string | null;
  };
};

/**
 * Actions for updating the launcher state.
 */
export type LauncherActions = {
  // Search
  setQuery: (q: string) => void;
  setGroups: (groups: Groups) => void;
  setLoading: (loading: boolean) => void;

  // Selection
  setSelectedIds: (ids: Set<string>) => void;
  setSelectionMode: (mode: SelectionMode) => void;
  setCursorIndex: (index: number) => void;
  moveCursor: (delta: number) => void;
  flatItems: () => string[];
  cursorId: () => string | null;

  // UI
  setPlaceholder: (text: string) => void;
  setViewStack: (stack: ViewStackEntry[]) => void;
  showActionMenu: (actions: ActionInfo[]) => void;
  hideActionMenu: () => void;

  // Feedback
  setProgress: (msg: string | null) => void;
  setComplete: (msg: string | null) => void;
  setError: (msg: string | null) => void;

  // Helpers
  reset: () => void;
};

// =============================================================================
// Context
// =============================================================================

const LauncherContext = createContext<[LauncherState, LauncherActions]>();

/**
 * Initial state for the launcher.
 */
const createInitialState = (): LauncherState => ({
  search: {
    query: "",
    groups: [],
    loading: false,
  },
  selection: {
    ids: new Set<string>(),
    mode: "single",
    cursorIndex: 0,
  },
  ui: {
    placeholder: "Search...",
    viewStack: [],
    actionMenu: {
      visible: false,
      actions: [],
    },
  },
  feedback: {
    progress: null,
    complete: null,
    error: null,
  },
});

// =============================================================================
// Provider
// =============================================================================

/**
 * Provider component for the launcher store.
 * Wraps the app to provide global state management.
 */
export const LauncherProvider: ParentComponent = (props) => {
  const [state, setState] = createStore<LauncherState>(createInitialState());

  // =============================================================================
  // Actions
  // =============================================================================

  const actions: LauncherActions = {
    // Search actions
    setQuery: (q: string) => {
      setState("search", "query", q);
    },

    setGroups: (groups: Groups) => {
      // Use reconcile for efficient diffing when updating from backend
      setState("search", "groups", reconcile(groups));
    },

    setLoading: (loading: boolean) => {
      setState("search", "loading", loading);
    },

    // Selection actions
    setSelectedIds: (ids: Set<string>) => {
      setState("selection", "ids", ids);
    },

    setSelectionMode: (mode: SelectionMode) => {
      setState("selection", "mode", mode);
    },

    setCursorIndex: (index: number) => {
      const items = flattenGroups(state.search.groups);
      const clamped = Math.max(0, Math.min(index, Math.max(0, items.length - 1)));
      setState("selection", "cursorIndex", clamped);
    },

    moveCursor: (delta: number) => {
      const items = flattenGroups(state.search.groups);
      if (items.length === 0) return;
      const newIndex = state.selection.cursorIndex + delta;
      const clamped = Math.max(0, Math.min(newIndex, items.length - 1));
      setState("selection", "cursorIndex", clamped);
    },

    flatItems: () => flattenGroups(state.search.groups),

    cursorId: () => {
      const items = flattenGroups(state.search.groups);
      const index = state.selection.cursorIndex;
      return items[index] ?? null;
    },

    // UI actions
    setPlaceholder: (text: string) => {
      setState("ui", "placeholder", text);
    },

    setViewStack: (stack: ViewStackEntry[]) => {
      // Use reconcile for efficient diffing
      setState("ui", "viewStack", reconcile(stack));
    },

    showActionMenu: (actions: ActionInfo[]) => {
      // Batch updates to minimize re-renders
      batch(() => {
        setState("ui", "actionMenu", "actions", reconcile(actions));
        setState("ui", "actionMenu", "visible", true);
      });
    },

    hideActionMenu: () => {
      setState("ui", "actionMenu", "visible", false);
    },

    // Feedback actions
    setProgress: (msg: string | null) => {
      setState("feedback", "progress", msg);
    },

    setComplete: (msg: string | null) => {
      setState("feedback", "complete", msg);
    },

    setError: (msg: string | null) => {
      setState("feedback", "error", msg);
    },

    // Helper actions
    reset: () => {
      // Batch all state resets together
      batch(() => {
        setState("search", "query", "");
        setState("search", "groups", []);
        setState("search", "loading", false);
        setState("selection", "ids", new Set<string>());
        setState("selection", "mode", "single");
        setState("selection", "cursorIndex", 0);
        setState("ui", "placeholder", "Search...");
        setState("ui", "viewStack", []);
        setState("ui", "actionMenu", { visible: false, actions: [] });
        setState("feedback", "progress", null);
        setState("feedback", "complete", null);
        setState("feedback", "error", null);
      });
    },
  };

  // Clamp cursor when groups change (e.g., after search results update)
  createEffect(() => {
    const items = flattenGroups(state.search.groups);
    if (items.length === 0) {
      setState("selection", "cursorIndex", 0);
    } else if (state.selection.cursorIndex >= items.length) {
      setState("selection", "cursorIndex", items.length - 1);
    }
  });

  return (
    <LauncherContext.Provider value={[state, actions]}>
      {props.children}
    </LauncherContext.Provider>
  );
};

// =============================================================================
// Hook
// =============================================================================

/**
 * Hook to access the launcher store.
 * Must be used within a LauncherProvider.
 *
 * @returns Tuple of [state, actions]
 *
 * @example
 * ```tsx
 * const [state, actions] = useLauncher();
 *
 * // Read state
 * const query = state.search.query;
 *
 * // Update state
 * actions.setQuery("new query");
 * ```
 */
export const useLauncher = () => {
  const context = useContext(LauncherContext);
  if (!context) {
    throw new Error("useLauncher must be used within LauncherProvider");
  }
  return context;
};
