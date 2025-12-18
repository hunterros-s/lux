/**
 * Tauri API wrappers for type-safe communication with the Rust backend.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, emit, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  Groups,
  Item,
  ActionInfo,
  ActionResult,
  ViewState,
  ViewStackEntry,
} from './types';

// Re-export types for convenience
export type { ActionResult };

// =============================================================================
// Search Commands (TypeScript → Rust)
// =============================================================================

/**
 * Search for items matching the query.
 * Returns grouped results from all matching triggers and sources.
 *
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function search(query: string): Promise<Groups> {
  return invoke<Groups>('search', { query });
}

/**
 * Get view stack for breadcrumb display.
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function getViewStack(): Promise<ViewStackEntry[]> {
  const stack = await invoke<ViewState[]>('get_view_stack');
  // Convert ViewState to ViewStackEntry
  return stack.map((v, i) => ({
    title: v.title,
    index: i,
  }));
}

// =============================================================================
// Action Commands
// =============================================================================

/**
 * Get applicable actions for selected items.
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function getApplicableActions(items: Item[]): Promise<ActionInfo[]> {
  if (items.length === 0) return [];

  // Backend ActionInfo DTO format
  interface BackendActionInfo {
    plugin_name: string;
    action_index: number;
    id: string;
    title: string;
    icon?: string;
    bulk: boolean;
  }

  const result = await invoke<BackendActionInfo[]>('get_actions', { items });

  return result.map(a => ({
    pluginName: a.plugin_name,
    actionIndex: a.action_index,
    id: a.id,
    title: a.title,
    icon: a.icon,
    bulk: a.bulk,
  }));
}

/**
 * Execute an action on items.
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function executeAction(
  pluginName: string,
  actionIndex: number,
  items: Item[]
): Promise<ActionResult> {
  if (items.length === 0) return { type: 'None' };

  return invoke<ActionResult>('execute_action', {
    pluginName,
    actionIndex,
    items,
  });
}

/**
 * Execute the default (first applicable) action on items.
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function executeDefaultAction(items: Item[]): Promise<ActionResult> {
  if (items.length === 0) return { type: 'None' };

  return invoke<ActionResult>('execute_default_action', { items });
}

// =============================================================================
// View Stack Commands
// =============================================================================

/**
 * Pop the current view from the stack.
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function popView(): Promise<boolean> {
  return invoke<boolean>('pop_view');
}

/**
 * Pop to a specific view in the stack by index.
 * Uses the new QueryEngine (Plugin API v0.1).
 */
export async function popToView(index: number): Promise<void> {
  await invoke<void>('pop_to_view', { index });
}

/**
 * Dismiss the panel entirely.
 */
export async function dismiss(): Promise<void> {
  await emit('request-hide');
}

// =============================================================================
// Events (Rust → TypeScript)
// =============================================================================

/**
 * Listen for panel shown events.
 */
export function onPanelShown(callback: () => void): Promise<UnlistenFn> {
  return listen('lux:panel-shown', () => {
    callback();
  });
}

/**
 * Listen for panel hidden events.
 */
export function onPanelHidden(callback: () => void): Promise<UnlistenFn> {
  return listen('lux:panel-hidden', () => {
    callback();
  });
}

/**
 * Listen for results updates (from async sources).
 */
export function onResultsUpdated(callback: (groups: Groups) => void): Promise<UnlistenFn> {
  return listen('lux:results-updated', (event) => {
    callback(event.payload as Groups);
  });
}

/**
 * Listen for view state changes.
 */
export function onViewStateChanged(callback: (state: ViewState) => void): Promise<UnlistenFn> {
  return listen('lux:view-state-changed', (event) => {
    callback(event.payload as ViewState);
  });
}

/**
 * Listen for action progress updates.
 */
export function onActionProgress(callback: (message: string) => void): Promise<UnlistenFn> {
  return listen('lux:action-progress', (event) => {
    callback(event.payload as string);
  });
}

/**
 * Listen for action completion.
 */
export function onActionComplete(callback: (message: string) => void): Promise<UnlistenFn> {
  return listen('lux:action-complete', (event) => {
    callback(event.payload as string);
  });
}

/**
 * Listen for action failures.
 */
export function onActionFailed(callback: (error: string) => void): Promise<UnlistenFn> {
  return listen('lux:action-failed', (event) => {
    callback(event.payload as string);
  });
}

/**
 * Listen for plugin errors.
 */
export function onPluginError(callback: (error: string) => void): Promise<UnlistenFn> {
  return listen('lux:plugin-error', (event) => {
    callback(event.payload as string);
  });
}
