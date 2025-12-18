/**
 * Search hook for the Lux launcher.
 *
 * Manages search state and automatically triggers searches when query changes.
 */
import { createEffect, on } from "solid-js";
import { useLauncher } from "~/store";
import { search } from "~/commands/search";

/**
 * Hook for managing search functionality.
 *
 * Automatically triggers search when query changes.
 * Only exposes read-only accessors to prevent double-execution.
 *
 * @example
 * ```tsx
 * function SearchResults() {
 *   const { isSearching, results, query } = useSearch();
 *
 *   return (
 *     <Show when={!isSearching()} fallback={<Loading />}>
 *       <For each={results()}>
 *         {group => <Group group={group} />}
 *       </For>
 *     </Show>
 *   );
 * }
 * ```
 */
export function useSearch() {
  const [state, actions] = useLauncher();

  // Internal implementation - not exposed to prevent double-execution
  const executeSearch = async (query: string) => {
    // Empty query still triggers search (for showing default results)
    actions.setLoading(true);

    const result = await search(query);

    result.match(
      (groups) => {
        actions.setGroups(groups);
        actions.setLoading(false);
      },
      (error) => {
        actions.setError(error.message);
        actions.setLoading(false);
      }
    );
  };

  // Search effect - runs on initial mount and query changes
  createEffect(
    on(
      () => state.search.query,
      executeSearch
    )
  );

  // Only expose read-only accessors
  return {
    /** Whether a search is currently in progress */
    isSearching: () => state.search.loading,
    /** Current search results grouped */
    results: () => state.search.groups,
    /** Current search query */
    query: () => state.search.query,
  };
}
