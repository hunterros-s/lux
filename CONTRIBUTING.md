# Contributing

## Reporting Bugs

[Open a bug report](../../issues/new?template=bug_report.md) with:
- Steps to reproduce
- Expected vs actual behavior
- macOS version and Lux version

## Requesting Features

[Open a feature request](../../issues/new?template=feature_request.md) with:
- Description of the feature
- Use case

## Development Setup

Prerequisites:
- macOS 10.15+
- Rust (via [rustup](https://rustup.rs/))
- Node.js 18+

```bash
git clone https://github.com/hunterross/lux.git
cd lux
npm install
npm run tauri dev
```

## Running Tests

```bash
npm run test:run           # Frontend tests
cargo test -p lux          # Rust tests
```

## Code Style

- Rust: `cargo fmt` before committing
- TypeScript: Follow existing patterns
