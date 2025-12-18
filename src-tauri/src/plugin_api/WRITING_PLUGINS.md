# Plugin API Reference

API reference for the Lux Plugin API. Assumes familiarity with the codebase.

## Plugin Registration

```lua
lux.register({
    name = "my-plugin",       -- required, unique identifier
    triggers = { ... },       -- optional
    sources = { ... },        -- optional
    actions = { ... },        -- optional
    setup = function(config)  -- optional, called by lux.configure()
        -- Initialize with user config
    end,
})
```

Parsed by `lua/parse.rs:parse_plugin()`. Stored in `PluginRegistry`.

## Triggers

Triggers intercept queries before they reach the current view's source.

```lua
{
    -- One of these required:
    prefix = "=",                       -- fast prefix match
    match = function(ctx) return bool end,  -- custom match logic

    -- Required:
    run = function(ctx)
        ctx:set_items({ ... })  -- or ctx:set_groups({ ... })
        ctx:push({ ... })       -- push a view
        ctx:dismiss()           -- close launcher
    end,
}
```

### Trigger Context Fields

| Field | Type | Description |
|-------|------|-------------|
| `ctx.query` | string | Full query string |
| `ctx.args` | string | Query after prefix (if prefix trigger) |

### Trigger Context Methods

| Method | Description |
|--------|-------------|
| `ctx:set_items(items)` | Set results (wrapped in ungrouped group) |
| `ctx:set_groups(groups)` | Set grouped results |
| `ctx:push(view)` | Push a new view |
| `ctx:replace(view)` | Replace current view |
| `ctx:dismiss()` | Close launcher |

Implementation: `context.rs:TriggerContext`, `lua/bridge.rs:TriggerContextLua`

## Sources

Sources provide search results.

```lua
{
    name = "files",           -- optional, for debugging
    root = true,              -- contributes to root view
    group = "Recent Files",   -- group title in root view
    debounce_ms = 100,        -- wait after typing stops
    min_query_length = 2,     -- minimum chars before searching

    search = function(ctx)
        ctx:set_items({
            { id = "1", title = "File 1", types = {"file"} },
        })
        -- or ctx:set_groups({ ... })
    end,
}
```

### Source Context Fields

| Field | Type | Description |
|-------|------|-------------|
| `ctx.query` | string | Search query |
| `ctx.view_data` | table | Data passed when view was pushed |

### Source Context Methods

| Method | Description |
|--------|-------------|
| `ctx:set_items(items)` | Set results |
| `ctx:set_groups(groups)` | Set grouped results |

Note: Sources cannot push views or dismiss. They only return items.

Implementation: `context.rs:SourceContext`, `lua/bridge.rs:SourceContextLua`

## Actions

Actions operate on selected items.

```lua
{
    id = "open",              -- unique within plugin
    title = "Open",           -- display text
    icon = "open-icon",       -- optional
    bulk = false,             -- appears for multi-select?

    applies = function(ctx)
        return ctx.item.types[1] == "file"
    end,

    run = function(ctx)
        local item = ctx.item  -- or ctx.items for bulk
        -- do something with item
        ctx:dismiss()          -- close launcher
        -- or ctx:complete("Opened!") / ctx:fail("Error")
    end,
}
```

### Action Applies Context

Simple table (not typestate):

| Field | Type | Description |
|-------|------|-------------|
| `ctx.item` | table | The item being tested |

### Action Run Context Fields

| Field | Type | Description |
|-------|------|-------------|
| `ctx.item` | table | First item (convenience) |
| `ctx.items` | table | All selected items |
| `ctx.view_data` | table | View data |

### Action Run Context Methods

| Method | Description |
|--------|-------------|
| `ctx:push(view)` | Push a new view |
| `ctx:replace(view)` | Replace current view |
| `ctx:pop()` | Return to previous view |
| `ctx:dismiss()` | Close launcher |
| `ctx:progress(msg)` | Show progress indicator |
| `ctx:complete(msg)` | Mark as complete |
| `ctx:fail(error)` | Mark as failed |

Implementation: `context.rs:ActionContext`, `lua/bridge.rs:ActionContextLua`

## Views

Views are search contexts with their own source and selection behavior.

```lua
{
    title = "Select Files",
    placeholder = "Search files...",

    source = function(ctx)
        ctx:set_items({ ... })
    end,

    selection = "single",  -- "single" | "multi" | "custom"

    -- Required if selection = "custom":
    on_select = function(ctx)
        if ctx:is_selected(ctx.item.id) then
            ctx:deselect(ctx.item.id)
        else
            ctx:select(ctx.item.id)
        end
    end,

    on_submit = function(ctx)
        -- Called when user presses Enter
        ctx:dismiss()
    end,

    view_data = { foo = "bar" },  -- passed to source/callbacks

    keys = {
        ["ctrl+a"] = function(ctx) ... end,
        ["ctrl+d"] = "delete-action-id",
    },
}
```

### Selection Modes

| Mode | Behavior |
|------|----------|
| `single` | Selecting clears previous selection |
| `multi` | Toggle selection on each item |
| `custom` | `on_select` hook controls all selection logic |

### Select Context Fields/Methods

| Field/Method | Description |
|--------------|-------------|
| `ctx.item` | Item being selected |
| `ctx.view_data` | View data |
| `ctx:is_selected(id)` | Check if item selected |
| `ctx:get_selection()` | Get all selected IDs |
| `ctx:select(id)` | Select an item |
| `ctx:deselect(id)` | Deselect an item |
| `ctx:clear_selection()` | Clear all selection |

### Submit Context Fields/Methods

| Field/Method | Description |
|--------------|-------------|
| `ctx.query` | Current query |
| `ctx.view_data` | View data |
| `ctx:push(view)` | Push a view |
| `ctx:replace(view)` | Replace view |
| `ctx:pop()` | Pop view |
| `ctx:dismiss()` | Close launcher |

Implementation: `types.rs:View`, `lua/parse.rs:parse_view()`

## Item Structure

```lua
{
    id = "unique-id",         -- optional, auto-generated if missing
    title = "Display Title",  -- required
    subtitle = "Secondary",   -- optional
    icon = "icon-path",       -- optional
    types = {"file", "ts"},   -- for action filtering
    data = { path = "/..." }, -- arbitrary data for actions
}
```

Rust type: `types.rs:Item`

## Group Structure

```lua
{
    title = "Section Title",  -- optional, nil = ungrouped
    items = { ... },          -- array of items
}
```

Rust type: `types.rs:Group`

## Built-in Utilities

### lux.shell(command, opts?)

Execute a shell command with timeout.

```lua
local result = lux.shell("ls -la", {
    timeout_ms = 5000,  -- default 30000
    cwd = "/path",      -- optional working directory
})

result.stdout    -- string
result.stderr    -- string
result.exit_code -- number
result.success   -- boolean
result.timed_out -- boolean
```

### lux.icon(app_path)

Extract app icon as base64 data URL (macOS only).

```lua
local icon = lux.icon("/Applications/Safari.app")
-- Returns: "data:image/png;base64,..."
```

### lux.configure(name, config)

Pass configuration to a registered plugin. Calls the plugin's `setup` function.

```lua
lux.configure("my-plugin", {
    setting1 = "value",
})
```

### lux.set_root_view(view)

Override the default root view.

```lua
lux.set_root_view({
    placeholder = "Custom search...",
    source = function(ctx) ... end,
})
```

## Lua Registry Keys

Functions are stored in Lua's named registry. Key format:

```
plugin:{name}:trigger:{index}:match
plugin:{name}:trigger:{index}:run
plugin:{name}:source:{index}:search
plugin:{name}:action:{index}:applies
plugin:{name}:action:{index}:run
view:source:{uuid}
view:on_select:{uuid}
view:on_submit:{uuid}
```

Generated by `lua/parse.rs:generate_function_key()`. Inline view functions use UUIDs and are tracked in `ViewInstance.registry_keys` for cleanup.

## Error Handling

Lua errors are caught and converted to `PluginError::Lua`. Check `error.rs` for error types.

Common errors:
- Missing required fields (name, run, search, etc.)
- Invalid selection mode
- `selection = "custom"` without `on_select`
