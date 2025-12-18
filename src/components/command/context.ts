import { createContext, useContext, type Accessor } from "solid-js"
import type { SelectionMode } from "~/core/types"

export interface CommandContext {
  // Cursor (focused item via keyboard navigation)
  selectedId: Accessor<string>
  hoveredId: Accessor<string>
  isKeyboardNav: Accessor<boolean>

  // Selection (multi-select mode)
  selectedIds: Accessor<Set<string>>
  selectionMode: Accessor<SelectionMode>

  // Items (from store)
  items: Accessor<string[]>

  // Cursor actions
  selectItem: (id: string) => void
  activateItem: (id: string) => void
  setHoveredId: (id: string) => void

  // Selection actions
  toggleSelection: (id: string) => void
  clearSelection: () => void
  selectAll: () => void
  isItemSelected: (id: string) => boolean
}

export const CommandContext = createContext<CommandContext>()

export function useCommand() {
  const ctx = useContext(CommandContext)
  if (!ctx) {
    throw new Error("useCommand must be used within a CommandList")
  }
  return ctx
}
