/**
 * Creates a debounced version of the provided function.
 * The debounced function delays invoking the function until after
 * the specified milliseconds have elapsed since the last time it was invoked.
 *
 * @param fn - The function to debounce
 * @param ms - The number of milliseconds to delay
 * @returns A debounced version of the function
 *
 * @example
 * ```ts
 * const debouncedSearch = debounce(search, 150);
 * debouncedSearch("query"); // Only executes after 150ms of no calls
 * ```
 */
export function debounce<T extends (...args: never[]) => unknown>(
  fn: T,
  ms: number
): (...args: Parameters<T>) => void {
  let timeoutId: ReturnType<typeof setTimeout>;
  return (...args: Parameters<T>) => {
    clearTimeout(timeoutId);
    timeoutId = setTimeout(() => fn(...args), ms);
  };
}
