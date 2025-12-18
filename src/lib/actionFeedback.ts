/**
 * Shared utilities for handling action feedback.
 *
 * Centralizes the pattern for updating feedback state based on action results
 * or Tauri events, eliminating duplication between useActions and useTauriEvents.
 */
import { CONFIG } from "./config";

/**
 * Feedback actions interface - subset of LauncherActions for feedback updates.
 */
export type FeedbackActions = {
  setProgress: (msg: string | null) => void;
  setComplete: (msg: string | null) => void;
  setError: (msg: string | null) => void;
};

/**
 * Handle progress feedback.
 */
export function handleProgress(actions: FeedbackActions, message: string): void {
  actions.setProgress(message);
}

/**
 * Handle completion feedback with auto-dismiss timeout.
 */
export function handleComplete(actions: FeedbackActions, message: string): void {
  actions.setProgress(null);
  actions.setComplete(message);
  setTimeout(() => actions.setComplete(null), CONFIG.feedback.timeoutMs);
}

/**
 * Handle error feedback.
 */
export function handleError(actions: FeedbackActions, error: string): void {
  actions.setProgress(null);
  actions.setError(error);
}
