import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { useSearch } from "../useSearch";
import { createTestWrapper } from "~/test/utils";

// Mock the tauri invoke
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("useSearch", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  test("starts searching immediately on mount", () => {
    const { result } = renderHook(() => useSearch(), {
      wrapper: createTestWrapper(),
    });

    // With no debounce, search triggers immediately on mount
    expect(result.isSearching()).toBe(true);
  });

  test("returns empty results initially", () => {
    const { result } = renderHook(() => useSearch(), {
      wrapper: createTestWrapper(),
    });

    expect(result.results()).toEqual([]);
  });

  test("returns empty query initially", () => {
    const { result } = renderHook(() => useSearch(), {
      wrapper: createTestWrapper(),
    });

    expect(result.query()).toBe("");
  });

  test("exposes read-only accessors", () => {
    const { result } = renderHook(() => useSearch(), {
      wrapper: createTestWrapper(),
    });

    // Should only have these three methods
    expect(typeof result.isSearching).toBe("function");
    expect(typeof result.results).toBe("function");
    expect(typeof result.query).toBe("function");

    // Should not expose executeSearch or other internals
    expect((result as Record<string, unknown>).executeSearch).toBeUndefined();
    expect((result as Record<string, unknown>).setQuery).toBeUndefined();
  });
});
