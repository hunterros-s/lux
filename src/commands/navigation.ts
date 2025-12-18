/**
 * Type-safe navigation command wrappers with Result error handling.
 */
import { ResultAsync } from 'neverthrow';
import * as tauri from '~/core/tauri';
import { toIPCError, type IPCError } from '~/lib/result';
import type { ViewStackEntry } from '~/core/types';

/**
 * Pop the current view from the stack.
 * Returns true if a view was popped, false if already at root.
 */
export function popView(): ResultAsync<boolean, IPCError> {
  return ResultAsync.fromPromise(
    tauri.popView(),
    toIPCError
  );
}

/**
 * Pop to a specific view in the stack by index.
 */
export function popToView(index: number): ResultAsync<void, IPCError> {
  return ResultAsync.fromPromise(
    tauri.popToView(index),
    toIPCError
  );
}

/**
 * Get the current view stack for breadcrumb display.
 */
export function getViewStack(): ResultAsync<ViewStackEntry[], IPCError> {
  return ResultAsync.fromPromise(
    tauri.getViewStack(),
    toIPCError
  );
}

/**
 * Dismiss the panel entirely.
 */
export function dismiss(): ResultAsync<void, IPCError> {
  return ResultAsync.fromPromise(
    tauri.dismiss(),
    toIPCError
  );
}
