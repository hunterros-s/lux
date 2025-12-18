/**
 * Actions hook for the Lux launcher.
 *
 * Manages action execution and feedback.
 */
import { useLauncher } from "~/store";
import { executeDefaultAction, executeAction, getActions } from "~/commands/actions";
import { handleProgress, handleComplete, handleError } from "~/lib/actionFeedback";
import type { Item, ActionInfo, ActionResult } from "~/core/types";

/**
 * Hook for managing action execution.
 *
 * Provides methods for executing actions and handling results.
 *
 * @example
 * ```tsx
 * function ActionButton() {
 *   const { runDefaultAction, runAction, fetchActions } = useActions();
 *
 *   const handleClick = async (items: Item[]) => {
 *     await runDefaultAction(items);
 *   };
 * }
 * ```
 */
export function useActions() {
  const [, actions] = useLauncher();

  /**
   * Handle action result and update feedback state.
   */
  const handleActionResult = (result: ActionResult) => {
    switch (result.type) {
      case "Progress":
        handleProgress(actions, result.message);
        break;
      case "Complete":
        handleComplete(actions, result.message);
        break;
      case "Fail":
        handleError(actions, result.error);
        break;
      case "Dismiss":
      case "Pushed":
      case "Replaced":
      case "Popped":
      case "None":
        // These are handled by view stack or no action needed
        break;
    }
  };

  /**
   * Execute the default action on the given items.
   */
  const runDefaultAction = async (items: Item[]): Promise<ActionResult> => {
    if (items.length === 0) {
      return { type: "None" };
    }

    const result = await executeDefaultAction(items);

    return result.match(
      (actionResult) => {
        handleActionResult(actionResult);
        return actionResult;
      },
      (error) => {
        actions.setError(error.message);
        return { type: "Fail", error: error.message } as ActionResult;
      }
    );
  };

  /**
   * Execute a specific action on the given items.
   */
  const runAction = async (
    pluginName: string,
    actionIndex: number,
    items: Item[]
  ): Promise<ActionResult> => {
    if (items.length === 0) {
      return { type: "None" };
    }

    const result = await executeAction(pluginName, actionIndex, items);

    return result.match(
      (actionResult) => {
        handleActionResult(actionResult);
        return actionResult;
      },
      (error) => {
        actions.setError(error.message);
        return { type: "Fail", error: error.message } as ActionResult;
      }
    );
  };

  /**
   * Fetch applicable actions for the given items.
   */
  const fetchActions = async (items: Item[]): Promise<ActionInfo[]> => {
    if (items.length === 0) {
      return [];
    }

    const result = await getActions(items);

    return result.match(
      (actionInfos) => actionInfos,
      (error) => {
        console.error("Failed to get actions:", error.message);
        return [];
      }
    );
  };

  /**
   * Show the action menu with actions for the given items.
   */
  const showActionsMenu = async (items: Item[]) => {
    const availableActions = await fetchActions(items);
    if (availableActions.length > 0) {
      actions.showActionMenu(availableActions);
    }
  };

  /**
   * Hide the action menu.
   */
  const hideActionsMenu = () => {
    actions.hideActionMenu();
  };

  /**
   * Execute a selected action from the action menu.
   */
  const executeSelectedAction = async (action: ActionInfo, items: Item[]) => {
    hideActionsMenu();
    return runAction(action.pluginName, action.actionIndex, items);
  };

  return {
    runDefaultAction,
    runAction,
    fetchActions,
    showActionsMenu,
    hideActionsMenu,
    executeSelectedAction,
  };
}
