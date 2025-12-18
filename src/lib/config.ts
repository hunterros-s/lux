/**
 * Centralized configuration constants for the Lux launcher.
 * Extract hardcoded values here to enable easy customization.
 */
export const CONFIG = {
  feedback: {
    /** How long success/error messages display (ms) */
    timeoutMs: 3000,
  },
  search: {
    /** Debounce delay for search input (ms) */
    debounceMs: 150,
    /** Placeholder text when no context */
    defaultPlaceholder: "Search...",
  },
  ui: {
    /** Animation duration for transitions (ms) */
    transitionMs: 200,
  },
} as const satisfies Record<string, Record<string, unknown>>;
