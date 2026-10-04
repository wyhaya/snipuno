# AGENTS.md

## Code Style

- No comments for obvious code
- For paths that are used multiple times within a mod, they should be imported using a `use` declaration
- By default, do not use `--release` flag
- When running checks or tests, use the smallest relevant scope

## Agreement

- Do not write UI tests for UI components
- Never proactively perform visual testing

## Packager

Due to permission issues, some features can only be used in the packaged app. Please only run when necessary

```bash
cargo build
cargo packager -f app
# Run as an App
cargo build && cargo packager -f app && open ./dist/Snipuno.app
```
