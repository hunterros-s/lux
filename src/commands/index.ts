/**
 * Type-safe Tauri command wrappers with Result error handling.
 *
 * All commands return ResultAsync<T, IPCError> for compile-time error handling.
 *
 * @example
 * ```ts
 * import { search } from '~/commands';
 *
 * const result = await search(query);
 * result.match(
 *   groups => actions.setGroups(groups),
 *   error => actions.setError(error.message)
 * );
 * ```
 */
export * from './search';
export * from './actions';
export * from './navigation';
