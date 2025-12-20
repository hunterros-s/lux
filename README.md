# Lux

A Spotlight-like launcher for macOS with a Lua plugin system.

**Early Development (v0.1.0)**: APIs are unstable and may change.

## Features

- Spotlight-style floating panel with global keyboard shortcut
- Lua plugin system for custom sources and actions
- View stack navigation for drilling into results
- Native macOS UI built with GPUI

## Requirements

- macOS 10.15+
- [Rust](https://rustup.rs/) 1.70+

## Installation

Build from source:

```bash
git clone https://github.com/hunterross/lux.git
cd lux
cargo build --release
```

The built binary is in `target/release/lux-ui`.

## Usage

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Cmd+Shift+Space | Toggle panel |
| Enter | Execute default action |
| Escape | Close panel / navigate back |
| Arrow Up/Down | Navigate results |

### Configuration

Lux loads `~/.config/lux/init.lua` on startup. Use this to register plugins.

```lua
lux.register_source({
  name = "example",
  search = function(query)
    return {{
      id = "1",
      title = "Hello " .. query,
      types = {"text"}
    }}
  end
})
```

## Development

### Project Structure

```
lux/
├── crates/
│   ├── lux-core/           # Core types (Item, Group, ActionResult)
│   ├── lux-plugin-api/     # Plugin system with Lua scripting
│   ├── lux-lua-runtime/    # Lua runtime thread and async execution
│   └── lux-ui/             # GPUI native frontend
└── Cargo.toml
```

### Running in Development

```bash
cargo run -p lux-ui
```

### Running Tests

```bash
cargo test
```

### Tech Stack

- UI: [GPUI](https://github.com/zed-industries/zed) (native Rust UI framework)
- Scripting: Lua 5.4 (mlua)

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

## License

MIT
