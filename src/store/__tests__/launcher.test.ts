import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { useLauncher } from "../launcher";
import { createTestWrapper } from "~/test/utils";

describe("useLauncher", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe("initial state", () => {
    test("has empty search state", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state] = result;

      expect(state.search.query).toBe("");
      expect(state.search.groups).toEqual([]);
      expect(state.search.loading).toBe(false);
    });

    test("has default selection state", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state] = result;

      expect(state.selection.ids.size).toBe(0);
      expect(state.selection.mode).toBe("single");
      expect(state.selection.cursorIndex).toBe(0);
    });

    test("has default UI state", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state] = result;

      expect(state.ui.placeholder).toBe("Search...");
      expect(state.ui.viewStack).toEqual([]);
      expect(state.ui.actionMenu.visible).toBe(false);
      expect(state.ui.actionMenu.actions).toEqual([]);
    });

    test("has empty feedback state", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state] = result;

      expect(state.feedback.progress).toBeNull();
      expect(state.feedback.complete).toBeNull();
      expect(state.feedback.error).toBeNull();
    });
  });

  describe("search actions", () => {
    test("setQuery updates query", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      actions.setQuery("test query");

      expect(state.search.query).toBe("test query");
    });

    test("setGroups updates groups", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      const groups = [
        {
          title: "Test Group",
          items: [
            {
              id: "1",
              title: "Test Item",
              types: ["test"],
            },
          ],
        },
      ];

      actions.setGroups(groups);

      expect(state.search.groups).toEqual(groups);
    });

    test("setLoading updates loading state", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      expect(state.search.loading).toBe(false);

      actions.setLoading(true);

      expect(state.search.loading).toBe(true);

      actions.setLoading(false);

      expect(state.search.loading).toBe(false);
    });
  });

  describe("selection actions", () => {
    test("setSelectedIds updates selection", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      const ids = new Set(["item1", "item2"]);
      actions.setSelectedIds(ids);

      expect(state.selection.ids).toEqual(ids);
    });

    test("setSelectionMode updates mode", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      actions.setSelectionMode("multi");

      expect(state.selection.mode).toBe("multi");
    });

    test("setCursorIndex updates cursor index", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      // First add some groups so cursor has valid range
      actions.setGroups([
        { title: "Test", items: [{ id: "1", title: "Item 1", types: [] }, { id: "2", title: "Item 2", types: [] }] },
      ]);

      actions.setCursorIndex(1);

      expect(state.selection.cursorIndex).toBe(1);

      actions.setCursorIndex(0);

      expect(state.selection.cursorIndex).toBe(0);
    });

    test("moveCursor moves cursor by delta", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      // Add groups so cursor has valid range
      actions.setGroups([
        { title: "Test", items: [{ id: "1", title: "Item 1", types: [] }, { id: "2", title: "Item 2", types: [] }, { id: "3", title: "Item 3", types: [] }] },
      ]);

      expect(state.selection.cursorIndex).toBe(0);

      actions.moveCursor(1);
      expect(state.selection.cursorIndex).toBe(1);

      actions.moveCursor(1);
      expect(state.selection.cursorIndex).toBe(2);

      // Should clamp at end
      actions.moveCursor(1);
      expect(state.selection.cursorIndex).toBe(2);

      // Move back
      actions.moveCursor(-1);
      expect(state.selection.cursorIndex).toBe(1);
    });
  });

  describe("UI actions", () => {
    test("setPlaceholder updates placeholder", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      actions.setPlaceholder("Type to search...");

      expect(state.ui.placeholder).toBe("Type to search...");
    });

    test("setViewStack updates view stack", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      const stack = [{ title: "Files", index: 0 }];
      actions.setViewStack(stack);

      expect(state.ui.viewStack).toEqual(stack);
    });

    test("showActionMenu shows menu with actions", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      const menuActions = [
        {
          id: "open",
          title: "Open",
          pluginName: "test-plugin",
          actionIndex: 0,
          bulk: false,
        },
      ];

      actions.showActionMenu(menuActions);

      expect(state.ui.actionMenu.visible).toBe(true);
      expect(state.ui.actionMenu.actions).toEqual(menuActions);
    });

    test("hideActionMenu hides the menu", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      // First show the menu
      actions.showActionMenu([{ id: "open", title: "Open", pluginName: "test", actionIndex: 0, bulk: false }]);
      expect(state.ui.actionMenu.visible).toBe(true);

      // Then hide it
      actions.hideActionMenu();

      expect(state.ui.actionMenu.visible).toBe(false);
    });
  });

  describe("feedback actions", () => {
    test("setProgress updates progress message", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      actions.setProgress("Loading...");

      expect(state.feedback.progress).toBe("Loading...");

      actions.setProgress(null);

      expect(state.feedback.progress).toBeNull();
    });

    test("setComplete updates complete message", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      actions.setComplete("Done!");

      expect(state.feedback.complete).toBe("Done!");

      actions.setComplete(null);

      expect(state.feedback.complete).toBeNull();
    });

    test("setError updates error message", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      actions.setError("Something went wrong");

      expect(state.feedback.error).toBe("Something went wrong");

      actions.setError(null);

      expect(state.feedback.error).toBeNull();
    });
  });

  describe("reset action", () => {
    test("resets all state to initial values", () => {
      const { result } = renderHook(() => useLauncher(), {
        wrapper: createTestWrapper(),
      });
      const [state, actions] = result;

      // Modify all state - first add groups with items so cursor can be set
      actions.setGroups([{ title: "Test", items: [{ id: "item1", title: "Item 1", types: [] }] }]);
      actions.setQuery("test");
      actions.setLoading(true);
      actions.setSelectedIds(new Set(["item1"]));
      actions.setSelectionMode("multi");
      actions.setCursorIndex(0);
      actions.setPlaceholder("Custom placeholder");
      actions.setViewStack([{ title: "Files", index: 0 }]);
      actions.showActionMenu([{ id: "open", title: "Open", pluginName: "test", actionIndex: 0, bulk: false }]);
      actions.setProgress("Loading...");
      actions.setComplete("Done!");
      actions.setError("Error!");

      // Reset
      actions.reset();

      // Verify all state is reset
      expect(state.search.query).toBe("");
      expect(state.search.groups).toEqual([]);
      expect(state.search.loading).toBe(false);
      expect(state.selection.ids.size).toBe(0);
      expect(state.selection.mode).toBe("single");
      expect(state.selection.cursorIndex).toBe(0);
      expect(state.ui.placeholder).toBe("Search...");
      expect(state.ui.viewStack).toEqual([]);
      expect(state.ui.actionMenu.visible).toBe(false);
      expect(state.ui.actionMenu.actions).toEqual([]);
      expect(state.feedback.progress).toBeNull();
      expect(state.feedback.complete).toBeNull();
      expect(state.feedback.error).toBeNull();
    });
  });

  describe("error handling", () => {
    test("throws when used outside provider", () => {
      // This test verifies the hook throws when used without a provider
      // We need to suppress console.error for this test
      const consoleSpy = vi.spyOn(console, "error").mockImplementation(() => {});

      expect(() => {
        renderHook(() => useLauncher());
      }).toThrow("useLauncher must be used within LauncherProvider");

      consoleSpy.mockRestore();
    });
  });
});
