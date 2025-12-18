# Lux

A Spotlight-like launcher for macOS with a Lua plugin system.

**Early Development (v0.1.0)**: APIs are unstable and may change.

## Features

- Spotlight-style floating panel with global keyboard shortcut
- Lua plugin system for custom sources and actions
- View stack navigation for drilling into results

## Requirements

- macOS 10.15+
- [Rust](https://rustup.rs/) 1.70+
- [Node.js](https://nodejs.org/) 18+

## Installation

Build from source:

```bash
git clone https://github.com/hunterross/lux.git
cd lux
npm install
npm run tauri build
```

The built app is in `src-tauri/target/release/bundle/macos/`.

## Usage

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| Cmd+Space+Shift | Toggle panel |
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
├── src/                    # Solid.js frontend
│   ├── components/         # UI components
│   ├── features/           # Feature modules
│   ├── store/              # State management
│   └── core/               # Tauri API wrappers
├── src-tauri/              # Rust backend
│   └── src/
│       ├── lib.rs          # App entry point
│       ├── commands.rs     # Tauri commands
│       ├── plugin_api/     # Lua plugin system
│       └── platform/       # macOS-specific code
└── package.json
```

### Running in Development

```bash
npm run tauri dev
```

### Running Tests

```bash
npm run test:run           # Frontend tests
cargo test -p lux          # Rust tests
```

### Tech Stack

- Frontend: Solid.js, TypeScript, Vite, Tailwind CSS
- Backend: Tauri 2, Rust
- Scripting Language: Lua 5.4 (mlua)

## Contributing

See [CONTRIBUTING.md](./CONTRIBUTING.md).

## License

MIT
