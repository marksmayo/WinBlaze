# Contributing

## Coding Standards

- Prefer clear, explicit Rust over clever abstractions.
- Keep public APIs small and documented where the intent is not obvious.
- Avoid `unsafe` unless it is isolated, justified, and reviewed.
- Keep module names aligned with the primary type or responsibility they expose.
- Use ASCII by default in source files unless a file already requires Unicode.

## Formatting

- Format Rust code with `cargo fmt --all`.
- Keep lines near the repository `rustfmt.toml` target width.
- Let `rustfmt` manage import ordering and wrapping.

## Linting

- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Treat new warnings as build failures.
- Prefer targeted `#[allow(...)]` attributes only when there is a documented reason.

## Testing

- Add unit tests for deterministic logic in the owning crate.
- Add integration tests for crate boundaries, persistence, and FFI surfaces.
- Name tests after behavior, not implementation details.
- Keep regression tests small and focused on the failure mode they cover.

## Workspace Conventions

- Shared domain types live in `winblaze-core`.
- Scanner-specific scheduling and control logic live in `winblaze-scanner`.
- Persistence and index schema live in `winblaze-index`.
- Native UI boundary code lives in `winblaze-native`.
- Cross-crate behavior belongs in `tests/WinBlaze.Tests`.
