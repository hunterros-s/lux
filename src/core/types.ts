/**
 * Core domain types for Lux launcher.
 * These mirror the Rust plugin_api types for type-safe communication.
 */

// =============================================================================
// Item Types (Plugin System)
// =============================================================================

/**
 * A single item in any list.
 * This is the universal UI primitive that plugins return.
 */
export interface Item {
  id: string;
  title: string;
  subtitle?: string;
  icon?: string;
  /** Type tags for this item (e.g., ["file", "typescript"]) */
  types: string[];
  /** Plugin-specific data */
  data?: unknown;
}

/**
 * A group of items with an optional title.
 */
export interface Group {
  /** Optional group title (e.g., "Recent Files", "Applications") */
  title?: string;
  items: Item[];
}

/**
 * Array of groups - the standard return type from sources.
 */
export type Groups = Group[];

// =============================================================================
// Selection Types
// =============================================================================

/**
 * Selection mode for a view.
 */
export type SelectionMode = 'single' | 'multi' | 'custom';

/**
 * Current view state sent from backend.
 */
export interface ViewState {
  /** View title (for breadcrumbs) */
  title?: string;
  /** Input placeholder text */
  placeholder?: string;
  /** Current selection mode */
  selectionMode: SelectionMode;
  /** Currently selected item IDs */
  selectedIds: string[];
  /** Currently focused (cursor) item ID */
  cursorId?: string;
  /** Current query */
  query: string;
}

// =============================================================================
// Action Types
// =============================================================================

/**
 * Information about an available action.
 */
export interface ActionInfo {
  /** Plugin that owns this action */
  pluginName: string;
  /** Action index within the plugin */
  actionIndex: number;
  /** Action identifier */
  id: string;
  /** Display title */
  title: string;
  /** Optional icon */
  icon?: string;
  /** Whether this action supports bulk operations */
  bulk: boolean;
}

/**
 * Follow-up action for action completion.
 */
export interface FollowUpAction {
  title: string;
  action: () => void;
}

/**
 * Result from action execution.
 */
export type ActionResult =
  | { type: 'Dismiss' }
  | { type: 'Pushed' }
  | { type: 'Replaced' }
  | { type: 'Popped' }
  | { type: 'Progress'; message: string }
  | { type: 'Complete'; message: string; actions?: FollowUpAction[] }
  | { type: 'Fail'; error: string }
  | { type: 'None' };

// =============================================================================
// View Stack Types
// =============================================================================

/**
 * Entry in the view stack for breadcrumb display.
 */
export interface ViewStackEntry {
  title?: string;
  index: number;
}

// =============================================================================
// Event Payload Types
// =============================================================================

/**
 * Payload for the lux:panel-shown event.
 */
export interface PanelShownPayload {
  results?: Groups;
}
