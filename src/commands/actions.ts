/**
 * Type-safe action command wrappers with Result error handling.
 */
import { ResultAsync } from 'neverthrow';
import * as tauri from '~/core/tauri';
import { toIPCError, type IPCError } from '~/lib/result';
import type { Item, ActionInfo, ActionResult } from '~/core/types';

/**
 * Get applicable actions for selected items.
 */
export function getActions(items: Item[]): ResultAsync<ActionInfo[], IPCError> {
  return ResultAsync.fromPromise(
    tauri.getApplicableActions(items),
    toIPCError
  );
}

/**
 * Execute an action on items.
 */
export function executeAction(
  pluginName: string,
  actionIndex: number,
  items: Item[]
): ResultAsync<ActionResult, IPCError> {
  return ResultAsync.fromPromise(
    tauri.executeAction(pluginName, actionIndex, items),
    toIPCError
  );
}

/**
 * Execute the default (first applicable) action on items.
 */
export function executeDefaultAction(items: Item[]): ResultAsync<ActionResult, IPCError> {
  return ResultAsync.fromPromise(
    tauri.executeDefaultAction(items),
    toIPCError
  );
}
