import { ParentProps } from "solid-js";
import { LauncherProvider } from "~/store/launcher";

/**
 * Creates a test wrapper component that provides the LauncherProvider context.
 * Use this with renderHook and render from @solidjs/testing-library.
 *
 * @example
 * ```tsx
 * const { result } = renderHook(() => useSearch(), {
 *   wrapper: createTestWrapper()
 * });
 * ```
 */
export function createTestWrapper() {
  return (props: ParentProps) => (
    <LauncherProvider>
      {props.children}
    </LauncherProvider>
  );
}

/**
 * Helper to wait for a condition to be true.
 * Useful for testing async effects.
 */
export async function waitForCondition(
  condition: () => boolean,
  timeout = 1000,
  interval = 10
): Promise<void> {
  const start = Date.now();
  while (!condition()) {
    if (Date.now() - start > timeout) {
      throw new Error('Condition not met within timeout');
    }
    await new Promise(resolve => setTimeout(resolve, interval));
  }
}
