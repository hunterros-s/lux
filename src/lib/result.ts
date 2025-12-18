/**
 * Type-safe error handling utilities using neverthrow.
 *
 * Provides a consistent pattern for wrapping async operations
 * with Result types for compile-time error handling.
 */
import { ResultAsync } from 'neverthrow';

/**
 * IPC error type for Tauri command failures.
 */
export type IPCError = {
  type: 'ipc_error';
  code: string;
  message: string;
};

/**
 * Convert an unknown error to an IPCError.
 */
export const toIPCError = (e: unknown): IPCError => ({
  type: 'ipc_error',
  code: 'INVOKE_FAILED',
  message: e instanceof Error ? e.message : String(e),
});

/**
 * Wrap an async function with Result type error handling.
 *
 * @example
 * ```ts
 * const result = await wrapAsync(() => search(query));
 * result.match(
 *   groups => handleSuccess(groups),
 *   error => handleError(error.message)
 * );
 * ```
 */
export function wrapAsync<T>(fn: () => Promise<T>): ResultAsync<T, IPCError> {
  return ResultAsync.fromPromise(fn(), toIPCError);
}
