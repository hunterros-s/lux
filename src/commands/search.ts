/**
 * Type-safe search command wrappers with Result error handling.
 */
import { ResultAsync } from 'neverthrow';
import * as tauri from '~/core/tauri';
import { toIPCError, type IPCError } from '~/lib/result';
import type { Groups } from '~/core/types';

/**
 * Search for items matching the query.
 * Returns a Result wrapping grouped results from all matching triggers and sources.
 */
export function search(query: string): ResultAsync<Groups, IPCError> {
  return ResultAsync.fromPromise(
    tauri.search(query),
    toIPCError
  );
}
