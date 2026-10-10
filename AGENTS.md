# Repository Guidelines

## Project Structure & Module Organization

WinBlaze is a Windows disk usage analyzer with a Rust backend and C++/WinRT WinUI 3 frontend. The Cargo workspace contains `src/WinBlaze.Core` (domain models and rollups), `WinBlaze.Scanner` (NTFS and directory scanning), `WinBlaze.Index` (persistence and search), `WinBlaze.Native` (C ABI bridge and scan CLI), and `WinBlaze.Updater`. Keep changes in the crate responsible for that behavior.

UI code lives in `src/WinBlaze.UI/src`, with assets in `src/WinBlaze.UI/Assets`. Cross-crate tests belong in `tests/WinBlaze.Tests`; PowerShell UI automation lives in `tests/ui`. Use `benchmarks/` for performance datasets and budgets, `scripts/` for build and packaging automation, `installer/` for MSI definitions, and `docs/` for architecture and setup guidance.

## Build, Test, and Development Commands

Run commands from the repository root on Windows. Use the pinned Rust 1.96.0 toolchain, Visual Studio C++ tools, and Windows SDK; see `docs/DEVELOPER_SETUP.md`.

- `cargo build --release -p winblaze-native`: build the optimized native engine and CLI.
- `cargo test --workspace`: run Rust unit and integration tests.
- `cargo test --workspace --examples`: validate example targets.
- `cargo fmt --all --check`: check formatting; use `cargo fmt --all` to apply it.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: enforce warning-free Rust linting.
- `msbuild src\WinBlaze.UI\WinBlaze.UI.vcxproj /restore /p:Configuration=Debug /p:Platform=x64`: build the UI from a Visual Studio developer shell. Launch `src\WinBlaze.UI\bin\x64\Debug\WinBlaze.UI.exe`.
- `powershell.exe -ExecutionPolicy Bypass -File scripts\check-local.ps1 -Configuration Debug`: run the local validation gate before committing.

## Coding Style & Naming Conventions

Use four-space indentation. Let rustfmt manage Rust imports and wrapping at the configured 100-column width. Use `snake_case` for Rust functions/modules and `PascalCase` for types; follow existing C++ naming and brace placement. Keep public APIs small, document unclear intent, and isolate and justify `unsafe` code. Prefer ASCII in source files.

## Testing Guidelines

Use Rust's built-in `#[test]` framework; name tests after observable behavior. Add focused regressions in the owning crate and boundary tests under `tests/WinBlaze.Tests`. CI requires 92% Rust line coverage through `scripts/check-coverage.ps1`, with documented FFI/raw-volume exclusions. UI changes require relevant `tests/ui` checks; performance changes must respect `benchmarks/performance-budgets*.json`.

## Commit & Pull Request Guidelines

Recent commits use concise imperative subjects, such as `Improve scanner hot path performance`. Follow that style. PRs should explain the problem, resulting behavior, and validation performed; link related issues and include screenshots for visible UI changes. Document benchmark evidence for performance claims.
