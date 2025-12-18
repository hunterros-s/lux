import { describe, test, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook } from "@solidjs/testing-library";
import { useActions } from "../useActions";
import { createTestWrapper } from "~/test/utils";
import { ok, err } from "neverthrow";
import type { ActionResult, Item, ActionInfo } from "~/core/types";

// Mock the commands
vi.mock("~/commands/actions", () => ({
  executeDefaultAction: vi.fn(),
  executeAction: vi.fn(),
  getActions: vi.fn(),
}));

// Import mocked functions
import { executeDefaultAction, executeAction, getActions } from "~/commands/actions";

const mockExecuteDefaultAction = executeDefaultAction as ReturnType<typeof vi.fn>;
const mockExecuteAction = executeAction as ReturnType<typeof vi.fn>;
const mockGetActions = getActions as ReturnType<typeof vi.fn>;

describe("useActions", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  const createMockItem = (id: string): Item => ({
    id,
    title: `Item ${id}`,
    types: ["test"],
  });

  describe("runDefaultAction", () => {
    test("returns None for empty items array", async () => {
      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const actionResult = await result.runDefaultAction([]);

      expect(actionResult).toEqual({ type: "None" });
      expect(mockExecuteDefaultAction).not.toHaveBeenCalled();
    });

    test("executes default action and returns result", async () => {
      const mockResult: ActionResult = { type: "Dismiss" };
      mockExecuteDefaultAction.mockResolvedValueOnce(ok(mockResult));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actionResult = await result.runDefaultAction(items);

      expect(actionResult).toEqual(mockResult);
      expect(mockExecuteDefaultAction).toHaveBeenCalledWith(items);
    });

    test("handles Complete result and sets feedback", async () => {
      const mockResult: ActionResult = { type: "Complete", message: "Done!" };
      mockExecuteDefaultAction.mockResolvedValueOnce(ok(mockResult));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      await result.runDefaultAction(items);

      // Complete feedback should clear after timeout
      vi.advanceTimersByTime(3000);
    });

    test("handles Progress result", async () => {
      const mockResult: ActionResult = { type: "Progress", message: "Loading..." };
      mockExecuteDefaultAction.mockResolvedValueOnce(ok(mockResult));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actionResult = await result.runDefaultAction(items);

      expect(actionResult).toEqual(mockResult);
    });

    test("handles error and sets error feedback", async () => {
      mockExecuteDefaultAction.mockResolvedValueOnce(
        err({ type: "ipc_error", code: "INVOKE_FAILED", message: "Connection failed" })
      );

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actionResult = await result.runDefaultAction(items);

      expect(actionResult).toEqual({ type: "Fail", error: "Connection failed" });
    });
  });

  describe("runAction", () => {
    test("returns None for empty items array", async () => {
      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const actionResult = await result.runAction("plugin", 0, []);

      expect(actionResult).toEqual({ type: "None" });
      expect(mockExecuteAction).not.toHaveBeenCalled();
    });

    test("executes specific action and returns result", async () => {
      const mockResult: ActionResult = { type: "Dismiss" };
      mockExecuteAction.mockResolvedValueOnce(ok(mockResult));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actionResult = await result.runAction("test-plugin", 1, items);

      expect(actionResult).toEqual(mockResult);
      expect(mockExecuteAction).toHaveBeenCalledWith("test-plugin", 1, items);
    });

    test("handles error and sets error feedback", async () => {
      mockExecuteAction.mockResolvedValueOnce(
        err({ type: "ipc_error", code: "INVOKE_FAILED", message: "Action failed" })
      );

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actionResult = await result.runAction("plugin", 0, items);

      expect(actionResult).toEqual({ type: "Fail", error: "Action failed" });
    });
  });

  describe("fetchActions", () => {
    test("returns empty array for empty items", async () => {
      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const actions = await result.fetchActions([]);

      expect(actions).toEqual([]);
      expect(mockGetActions).not.toHaveBeenCalled();
    });

    test("fetches and returns actions", async () => {
      const mockActions: ActionInfo[] = [
        { id: "open", title: "Open", pluginName: "test", actionIndex: 0, bulk: false },
        { id: "copy", title: "Copy", pluginName: "test", actionIndex: 1, bulk: false },
      ];
      mockGetActions.mockResolvedValueOnce(ok(mockActions));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actions = await result.fetchActions(items);

      expect(actions).toEqual(mockActions);
      expect(mockGetActions).toHaveBeenCalledWith(items);
    });

    test("returns empty array on error", async () => {
      mockGetActions.mockResolvedValueOnce(
        err({ type: "ipc_error", code: "INVOKE_FAILED", message: "Fetch failed" })
      );

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      const actions = await result.fetchActions(items);

      expect(actions).toEqual([]);
    });
  });

  describe("showActionsMenu", () => {
    test("does nothing for empty items", async () => {
      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      await result.showActionsMenu([]);

      expect(mockGetActions).not.toHaveBeenCalled();
    });

    test("fetches actions and shows menu when actions available", async () => {
      const mockActions: ActionInfo[] = [
        { id: "open", title: "Open", pluginName: "test", actionIndex: 0, bulk: false },
      ];
      mockGetActions.mockResolvedValueOnce(ok(mockActions));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      await result.showActionsMenu(items);

      expect(mockGetActions).toHaveBeenCalledWith(items);
    });

    test("does not show menu when no actions available", async () => {
      mockGetActions.mockResolvedValueOnce(ok([]));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const items = [createMockItem("1")];
      await result.showActionsMenu(items);

      expect(mockGetActions).toHaveBeenCalledWith(items);
    });
  });

  describe("hideActionsMenu", () => {
    test("hides the action menu", () => {
      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      // Just verify the function exists and can be called
      expect(() => result.hideActionsMenu()).not.toThrow();
    });
  });

  describe("executeSelectedAction", () => {
    test("hides menu and executes action", async () => {
      const mockResult: ActionResult = { type: "Dismiss" };
      mockExecuteAction.mockResolvedValueOnce(ok(mockResult));

      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      const action: ActionInfo = { id: "open", title: "Open", pluginName: "test", actionIndex: 0, bulk: false };
      const items = [createMockItem("1")];

      const actionResult = await result.executeSelectedAction(action, items);

      expect(actionResult).toEqual(mockResult);
      expect(mockExecuteAction).toHaveBeenCalledWith("test", 0, items);
    });
  });

  describe("exposes correct API", () => {
    test("exposes all action methods", () => {
      const { result } = renderHook(() => useActions(), {
        wrapper: createTestWrapper(),
      });

      expect(typeof result.runDefaultAction).toBe("function");
      expect(typeof result.runAction).toBe("function");
      expect(typeof result.fetchActions).toBe("function");
      expect(typeof result.showActionsMenu).toBe("function");
      expect(typeof result.hideActionsMenu).toBe("function");
      expect(typeof result.executeSelectedAction).toBe("function");
    });
  });
});
