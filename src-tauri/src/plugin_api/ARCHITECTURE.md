# Plugin API Architecture

This document describes the internal architecture of the Lux Plugin API for contributors.

## Module Overview

```
plugin_api/
├── mod.rs              # Re-exports, architecture diagram
├── types.rs            # Core types (Item, Group, Plugin, View, etc.)
├── error.rs            # PluginError enum
├── effect.rs           # Effect enum, EffectCollector, ViewSpec
├── context.rs          # Typestate contexts (TriggerContext, SourceContext, etc.)
├── handle.rs           # Opaque handles and type-specific registries
├── registry.rs         # PluginRegistry (stores plugins, components)
├── engine.rs           # QueryEngine (orchestrates execution)
├── engine/
│   └── engine_impl/    # Decomposed engine implementation
│       ├── actions.rs
│       ├── triggers.rs
│       ├── sources.rs
│       ├── selection.rs
│       ├── view_stack.rs
│       └── types.rs
└── lua/
    ├── mod.rs          # lux.register, lux.configure, utilities
    ├── bridge.rs       # Lua UserData wrappers, call_* functions
    └── parse.rs        # Lua table → Rust type parsing
```

## Effect-Based Execution

Lua callbacks don't mutate engine state directly. Instead, they describe *intent* via effects.

### Flow

```
1. Engine calls Lua function with a context
2. Lua code calls context methods (ctx:set_items(), ctx:push(), etc.)
3. Each method call pushes an Effect to the EffectCollector
4. After Lua returns, engine calls apply_effects() with collected effects
5. Engine mutates its own state based on effects
```

### Why Effects?

- **Single point of mutation**: All state changes go through `apply_effects()`
- **Validation**: Engine can validate/reject effects before applying
- **Testability**: Effects can be collected and inspected without side effects
- **Atomicity**: All effects from a callback are applied together

### Effect Types

```rust
pub enum Effect {
    SetGroups(Vec<Group>),  // Return search results
    PushView(ViewSpec),      // Push a new view
    ReplaceView(ViewSpec),   // Replace current view
    Pop,                     // Pop to previous view
    Dismiss,                 // Close launcher
    Progress(String),        // Show progress message
    Complete { message },    // Action completed
    Fail { error },          // Action failed
    Select(Vec<String>),     // Select items by ID
    Deselect(Vec<String>),   // Deselect items by ID
    ClearSelection,          // Clear all selection
}
```

### EffectCollector

Uses `RefCell<Vec<Effect>>` for interior mutability during a single Lua call:

```rust
let collector = EffectCollector::new();
let ctx = TriggerContext::new(query, args, &collector);
// ... Lua call happens ...
let effects = collector.take();  // Move out, no clone
engine.apply_effects(lua, effects);
```

## Typestate Contexts

Different callbacks have different capabilities. The type system enforces this.

### Context Capabilities

| Context | set_groups | push/replace | pop | dismiss | progress/complete/fail | selection |
|---------|-----------|--------------|-----|---------|----------------------|-----------|
| TriggerContext | ✓ | ✓ | ✗ | ✓ | ✗ | ✗ |
| SourceContext | ✓ | ✗ | ✗ | ✗ | ✗ | ✗ |
| ActionContext | ✗ | ✓ | ✓ | ✓ | ✓ | ✗ |
| SelectContext | ✗ | ✗ | ✗ | ✗ | ✗ | ✓ |
| SubmitContext | ✗ | ✓ | ✓ | ✓ | ✗ | ✗ |

### Why Typestate?

- **Compile-time safety**: Can't call `ctx:pop()` from a source callback
- **Self-documenting**: Looking at the context type tells you what's allowed
- **IDE support**: Autocomplete only shows valid methods

### Table-Based vs Typestate

Simple hooks (match, applies) use plain Lua tables since they only need read access:

```rust
pub fn build_trigger_match_context(lua: &Lua, query: &str) -> Table {
    let ctx = lua.create_table()?;
    ctx.set("query", query)?;
    Ok(ctx)
}
```

Effect-producing hooks use typestate contexts wrapped as Lua UserData.

## View Stack

The launcher maintains a stack of views. Root view is at the bottom; pushed views stack on top.

### ViewInstance

```rust
pub struct ViewInstance {
    pub view: View,                    // The view definition
    pub cursor_id: Option<String>,     // Currently focused item
    pub selected_ids: HashSet<String>, // Selected items
    pub query: String,                 // Current query
    pub scroll_position: Option<u32>,  // Preserved scroll
    pub registry_keys: Vec<String>,    // Lua keys to clean up on pop
}
```

### Lifecycle

1. **Push**: Create ViewInstance, add to stack, query resets
2. **Replace**: Pop current (cleanup keys), push new
3. **Pop**: Clean up registry keys, restore previous view's state
4. **Dismiss**: Close launcher entirely

### Root View

The root view aggregates all sources with `root = true`. It's created by `QueryEngine::initialize()`.

## Lua Registry Key Management

Lua functions can't be stored directly in Rust (they reference the Lua state). We store them in Lua's named registry.

### Key Format

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

### Inline Functions

When a trigger/action pushes a view with an inline source function:

```lua
ctx:push({
    source = function(ctx) ... end,  -- inline function
})
```

This creates a UUID-based key, stored in `ViewInstance.registry_keys`. When the view is popped, `cleanup_view_registry_keys()` removes them.

### LuaFunctionRef

```rust
pub struct LuaFunctionRef {
    pub key: String,  // Registry key
}

impl LuaFunctionRef {
    pub fn call<A, R>(&self, lua: &Lua, args: A) -> LuaResult<R> { ... }
    pub fn cleanup(&self, lua: &Lua) -> LuaResult<()> { ... }
}
```

## Thread Safety

### PluginRegistry

Uses `parking_lot::RwLock` for concurrent read access:

```rust
pub struct PluginRegistry {
    plugins: RwLock<HashMap<String, PluginEntry>>,
    triggers: RwLock<Vec<(String, TriggerEntry)>>,
    sources: RwLock<Vec<(String, SourceEntry)>>,
    actions: RwLock<Vec<(String, ActionEntry)>>,
    root_view: RwLock<Option<View>>,
}
```

Safe access via callbacks prevents lifetime issues:

```rust
registry.with_trigger("plugin", 0, |trigger| {
    // Use trigger here, lock released after closure
});
```

### QueryEngine

View stack uses `RwLock<Vec<ViewInstance>>`. Query generation uses `Mutex<u64>` for async cancellation.

### EffectCollector

Uses `RefCell` (not thread-safe) because it only exists during a single Lua call on a single thread.

## Query Execution Flow

```
search(query)
    │
    ├── Increment query generation
    ├── Update current view's query
    │
    ├── find_matching_triggers()
    │   ├── For each trigger: test prefix OR call match_fn
    │   └── Return Vec<(plugin_name, trigger_index)>
    │
    ├── For each matching trigger:
    │   ├── run_trigger() → Vec<Effect>
    │   └── apply_effects() → may push view, add results
    │
    ├── If no view pushed:
    │   ├── run_current_view_source() → Vec<Effect>
    │   └── Extract SetGroups → Groups
    │
    └── Return merged Groups
```

## Action Execution Flow

```
execute_action(plugin_name, action_index, items)
    │
    ├── Get action from registry
    ├── Create ActionContext with EffectCollector
    ├── Call action.run_fn via Lua
    ├── Collect effects
    ├── apply_effects()
    │   ├── PushView/ReplaceView → modify view stack
    │   ├── Pop → pop view stack
    │   ├── Dismiss → set dismissed flag
    │   ├── Progress/Complete/Fail → set result fields
    │   └── Selection effects → modify view state
    │
    └── Return ActionResult based on ApplyResult
```

## Registration Order

Components are stored in Vecs to preserve registration order. This matters for:

- **Triggers**: First matching trigger wins
- **Sources**: Results merged in registration order
- **Actions**: First applicable action is the default
