# Repository Guidelines

## Project Structure & Module Organization

This repository contains the `iced_richedit` Rust library, an advanced rich text editor widget for iced. The document core uses `prosemirror`; keep its types behind the `document` module instead of exposing them in the public API. Keep root-level files for Cargo metadata and project documentation. Use:

- `src/` for library code and public modules.
- `tests/` for tests mirroring `src/`.
- `assets/` for static resources and fixtures.
- `docs/` for architecture and contributor notes.

Organize code by feature or domain. Build small, cohesive modules with explicit responsibilities and straightforward control flow. Do not commit generated output unless distribution artifacts are intentionally versioned.

## Build, Test, and Development Commands

Use `cargo check` for fast compilation checks, `cargo test` for the test suite, `cargo fmt --check` for formatting, and `cargo clippy --all-targets --all-features -- -D warnings` for linting. Run all four before submitting work.

## Coding Style & Naming Conventions

Use stable `rustfmt` and Clippy. Follow Rust naming conventions: `snake_case` modules and functions, `CamelCase` types and traits, and `SCREAMING_SNAKE_CASE` constants. Choose descriptive identifiers.

Do not add speculative fallback paths, duplicate logic, redundant state, or compatibility layers without a demonstrated requirement. Maintain one clear source of truth and remove obsolete branches when behavior changes. All repository content—including code, identifiers, comments, documentation, configuration descriptions, commit messages, and pull request text—must be in English.

## Testing Guidelines

Add tests for every behavior change and bug fix. Place them in `tests/` or beside source files when the framework favors co-location. Name tests after observable behavior, for example `saves_document_when_content_changes`. Cover success paths, boundaries, and expected failures. Document any coverage threshold when a test runner is adopted.

Do not duplicate tests for behavior that `prosemirror` already documents and guarantees internally. Test this crate's wrappers, public behavior, error mapping, schema boundaries, and iced integration instead. Keep focused dependency-level tests only for compatibility assumptions that this crate relies on, known upstream defects, or regressions that could affect our API. When a dependency upgrade makes such a test redundant, remove or consolidate it.

## Commit & Pull Request Guidelines

No Git history currently establishes a commit convention. Use concise, imperative subjects such as `Add document autosave`, and separate unrelated changes. Pull requests must explain the problem and solution, list verification steps, and link relevant issues. Include screenshots for visible UI changes and call out migrations or configuration changes.

## Security & Agent Instructions

Never commit secrets or local environment files. Provide sanitized examples such as `.env.example` and pin dependencies with the ecosystem lockfile.

Agents must communicate in the language established by the current conversation, while keeping repository content in English. Treat this file as living policy: add durable constraints when they emerge, and periodically consolidate or remove obsolete guidance to keep it concise and authoritative.
