import {
  type JSX,
  type ParentProps,
  createSignal,
  createEffect,
  Show,
} from "solid-js"
import { CommandContext, useCommand } from "./context"
import type { SelectionMode } from "~/core/types"

// =============================================================================
// CommandList - Root container with keyboard handling
// =============================================================================

export interface CommandListProps extends ParentProps {
  onActivate: (id: string) => void
  onEscape?: () => void
  onSelectionChange?: (id: string) => void
  onToggleSelection?: (id: string) => void
  loop?: boolean
  class?: string
  style?: string | JSX.CSSProperties
  selectionMode?: SelectionMode
  selectedIds?: Set<string>
  // Required - items and cursor from store
  items: () => string[]
  cursorIndex: () => number
  onCursorChange: (index: number) => void
}

export function CommandList(props: CommandListProps) {
  const [hoveredId, setHoveredId] = createSignal("")
  const [isKeyboardNav, setIsKeyboardNav] = createSignal(false)
  const [selectedIds, setSelectedIds] = createSignal<Set<string>>(new Set())

  // Derive selected ID from cursor index
  const items = () => props.items()
  const selectedId = () => {
    const itemList = items()
    const idx = props.cursorIndex()
    return itemList[idx] ?? ""
  }

  // Sync external selectedIds with internal state
  createEffect(() => {
    if (props.selectedIds) {
      setSelectedIds(props.selectedIds)
    }
  })

  const selectionMode = () => props.selectionMode ?? 'single'

  const selectItem = (id: string) => {
    const idx = items().indexOf(id)
    if (idx >= 0) {
      props.onCursorChange(idx)
    }
    props.onSelectionChange?.(id)
  }

  const activateItem = (id: string) => {
    props.onActivate(id)
  }

  const toggleSelection = (id: string) => {
    if (selectionMode() === 'single') {
      selectItem(id)
      return
    }

    setSelectedIds((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
      } else {
        next.add(id)
      }
      return next
    })
    props.onToggleSelection?.(id)
  }

  const clearSelection = () => {
    setSelectedIds(new Set<string>())
  }

  const selectAll = () => {
    setSelectedIds(new Set(items()))
  }

  const isItemSelected = (id: string) => {
    return selectedIds().has(id)
  }

  const handleKeyDown = (e: KeyboardEvent) => {
    const itemList = items()
    const currentIndex = itemList.indexOf(selectedId())

    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault()
        setIsKeyboardNav(true)
        setHoveredId("")
        if (itemList.length === 0) return
        let nextIndex = currentIndex + 1
        if (nextIndex >= itemList.length) {
          nextIndex = props.loop ? 0 : itemList.length - 1
        }
        selectItem(itemList[nextIndex])
        break
      }
      case "ArrowUp": {
        e.preventDefault()
        setIsKeyboardNav(true)
        setHoveredId("")
        if (itemList.length === 0) return
        let prevIndex = currentIndex - 1
        if (prevIndex < 0) {
          prevIndex = props.loop ? itemList.length - 1 : 0
        }
        selectItem(itemList[prevIndex])
        break
      }
      case "Enter": {
        e.preventDefault()
        const id = selectedId()
        if (id) {
          activateItem(id)
        }
        break
      }
      case " ": {
        // Space toggles selection in multi-select mode
        if (selectionMode() !== 'single') {
          e.preventDefault()
          const id = selectedId()
          if (id) {
            toggleSelection(id)
          }
        }
        break
      }
      case "Escape": {
        e.preventDefault()
        props.onEscape?.()
        break
      }
      case "a": {
        // Cmd+A / Ctrl+A to select all in multi mode
        if ((e.metaKey || e.ctrlKey) && selectionMode() === 'multi') {
          e.preventDefault()
          selectAll()
        }
        break
      }
    }
  }

  const handleMouseMove = () => {
    if (isKeyboardNav()) {
      setIsKeyboardNav(false)
    }
  }

  const ctx = {
    selectedId,
    hoveredId,
    isKeyboardNav,
    selectedIds,
    selectionMode,
    items,
    selectItem,
    activateItem,
    setHoveredId,
    toggleSelection,
    clearSelection,
    selectAll,
    isItemSelected,
  }

  return (
    <CommandContext.Provider value={ctx}>
      <div class={props.class} style={props.style} onKeyDown={handleKeyDown} onMouseMove={handleMouseMove}>
        {props.children}
      </div>
    </CommandContext.Provider>
  )
}

// =============================================================================
// CommandInput - Search input
// =============================================================================

export interface CommandInputProps {
  value?: string
  onValueChange?: (value: string) => void
  placeholder?: string
  class?: string
  ref?: HTMLInputElement | ((el: HTMLInputElement) => void)
  autofocus?: boolean
}

export function CommandInput(props: CommandInputProps) {
  return (
    <div class="flex items-center border-b px-4 py-1" style="border-color: rgba(255, 255, 255, 0.1)">
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        class="mr-3 size-5 shrink-0 text-muted-foreground"
      >
        <path d="M10 10m-7 0a7 7 0 1 0 14 0a7 7 0 1 0 -14 0" />
        <path d="M21 21l-6 -6" />
      </svg>
      <input
        ref={props.ref}
        type="text"
        class={`flex h-11 w-full bg-transparent text-base outline-none placeholder:text-muted-foreground ${props.class ?? ""}`}
        placeholder={props.placeholder}
        value={props.value ?? ""}
        onInput={(e) => props.onValueChange?.(e.currentTarget.value)}
        autofocus={props.autofocus}
        // Disable WebKit/browser behaviors
        autocomplete="off"
        autocorrect="off"
        autocapitalize="off"
        spellcheck={false}
        data-form-type="other"
      />
    </div>
  )
}

// =============================================================================
// CommandItems - Scrollable list container with auto-scroll
// =============================================================================

export interface CommandItemsProps extends ParentProps {
  class?: string
}

export function CommandItems(props: CommandItemsProps) {
  let containerRef: HTMLDivElement | undefined
  const ctx = useCommand()

  // Auto-scroll to keep selected item visible
  createEffect(() => {
    const id = ctx.selectedId()
    if (!id || !containerRef) return

    const selectedEl = containerRef.querySelector(`[data-command-item-id="${id}"]`)
    if (selectedEl) {
      selectedEl.scrollIntoView({ block: "nearest", behavior: "smooth" })
    }
  })

  return (
    <div ref={containerRef} data-command-items class={`overflow-x-hidden ${props.class ?? ""}`}>
      {props.children}
    </div>
  )
}

// =============================================================================
// CommandItem - Individual selectable item
// =============================================================================

export interface CommandItemProps extends ParentProps {
  id: string
  disabled?: boolean
  class?: string
}

export function CommandItem(props: CommandItemProps) {
  const ctx = useCommand()

  const isSelected = () => ctx.selectedId() === props.id
  const isHovered = () => ctx.hoveredId() === props.id
  const isChecked = () => ctx.isItemSelected(props.id)

  const handleClick = () => {
    if (props.disabled) return

    if (ctx.selectionMode() === 'multi') {
      // In multi-select, click toggles selection
      ctx.toggleSelection(props.id)
    } else if (isSelected()) {
      // Already selected in single mode - activate
      ctx.activateItem(props.id)
    } else {
      // Not selected - select
      ctx.selectItem(props.id)
    }
  }

  // Prevent mousedown from stealing focus from input
  const handleMouseDown = (e: MouseEvent) => {
    e.preventDefault()
  }

  const handleMouseEnter = () => {
    // Ignore mouseenter during keyboard nav (scroll can trigger false mouseenter)
    if (!props.disabled && !ctx.isKeyboardNav()) {
      ctx.setHoveredId(props.id)
    }
  }

  const handleMouseLeave = () => {
    ctx.setHoveredId("")
  }

  return (
    <div
      data-command-item
      data-command-item-id={props.id}
      data-selected={isSelected()}
      data-hovered={isHovered()}
      data-checked={isChecked()}
      data-disabled={props.disabled}
      onMouseDown={handleMouseDown}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
      onClick={handleClick}
      class={props.class}
    >
      {props.children}
    </div>
  )
}

// =============================================================================
// CommandGroup - Section with heading
// =============================================================================

export interface CommandGroupProps extends ParentProps {
  heading?: JSX.Element | string
  class?: string
}

export function CommandGroup(props: CommandGroupProps) {
  return (
    <div class={props.class}>
      <Show when={props.heading}>
        <div class="px-2 py-1.5 text-xs font-medium text-muted-foreground">
          {props.heading}
        </div>
      </Show>
      {props.children}
    </div>
  )
}

// =============================================================================
// CommandEmpty - Empty state
// =============================================================================

export interface CommandEmptyProps extends ParentProps {
  class?: string
}

export function CommandEmpty(props: CommandEmptyProps) {
  const ctx = useCommand()

  return (
    <Show when={ctx.items().length === 0}>
      <div class={`py-6 text-center text-sm ${props.class ?? ""}`}>
        {props.children}
      </div>
    </Show>
  )
}

// Re-export context hook
export { useCommand } from "./context"
