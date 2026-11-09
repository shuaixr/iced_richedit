# Repository Guidelines

## Project Structure & Module Organization

This repository contains the `iced_richedit` Rust library, an advanced rich text editor widget for iced. The document core uses `prosemirror`; keep its types behind the `document` module instead of exposing them in the public API. Keep root-level files for Cargo metadata and project documentation. Use:

- `src/` for library code and public modules.
- `tests/` for tests mirroring `src/`.
- `assets/` for static resources and fixtures.
- `docs/` for architecture and contributor notes.

Organize code by feature or domain. Build small, cohesive modules with explicit responsibilities and straightforward control flow. Do not commit generated output unless distribution artifacts are intentionally versioned.

The long-term goal is to support arbitrary valid ProseMirror documents, including custom schemas, nodes, marks, attributes, and nesting. This is not a current compatibility claim. Keep schema validation and model preservation distinct from registered rendering and editing capabilities; never silently flatten unsupported content or discard its attributes. Extend capabilities incrementally without exposing ProseMirror types or building speculative frameworks.

## Build, Test, and Development Commands

Use `cargo check` for fast compilation checks, `cargo test --features renderer-tests --lib --tests` for the full library test suite, `cargo fmt --check` for formatting, and `cargo clippy --all-targets --all-features -- -D warnings` for linting. Run all four before submitting work. Plain `cargo test` omits the feature-gated renderer cache tests.

## Coding Style & Naming Conventions

Use stable `rustfmt` and Clippy. Follow Rust naming conventions: `snake_case` modules and functions, `CamelCase` types and traits, and `SCREAMING_SNAKE_CASE` constants. Choose descriptive identifiers.

Do not add speculative fallback paths, duplicate logic, redundant state, or compatibility layers without a demonstrated requirement. Maintain one clear source of truth and remove obsolete branches when behavior changes. Repository content—including code, identifiers, comments, public documentation, configuration descriptions, commit messages, and pull request text—must be in English. Internal development guides, implementation tutorials, and review notes may use the language established by the current conversation; code examples and identifiers must remain in English.

## Testing Guidelines

Add tests for every behavior change and bug fix. Place them in `tests/` or beside source files when the framework favors co-location. Name tests after observable behavior, for example `saves_document_when_content_changes`. Cover success paths, boundaries, and expected failures. Document any coverage threshold when a test runner is adopted.

Do not duplicate tests for behavior that `prosemirror` already documents and guarantees internally. Test this crate's wrappers, public behavior, error mapping, schema boundaries, and iced integration instead. Keep focused dependency-level tests only for compatibility assumptions that this crate relies on, known upstream defects, or regressions that could affect our API. When a dependency upgrade makes such a test redundant, remove or consolidate it.

## Commit & Pull Request Guidelines

No Git history currently establishes a commit convention. Use concise, imperative subjects such as `Add document autosave`, and separate unrelated changes. Pull requests must explain the problem and solution, list verification steps, and link relevant issues. Include screenshots for visible UI changes and call out migrations or configuration changes.

## Agent Editing Boundary

Except for explicitly requested formatting-only changes, agents must never create, modify, delete, move, rename, or apply patches to project code, tests, examples, build configuration, or dependency files on the user's behalf, even when explicitly asked. This includes copying and pasting code and using scripts, generators, temporary worktrees, or temporary project copies to perform those edits indirectly. Provide code snippets or proposed diffs in the conversation, identify the exact file and location, and explain why each change is needed. The user applies all non-formatting changes.

Agents may inspect files and run non-source-mutating checks against user-applied changes. Formatting-only edits, including manual formatting inside `serde_json::json!` and running `cargo fmt`, are allowed when explicitly requested by the user. Keep formatting within the requested scope and preserve behavior and data values. Otherwise, use `cargo fmt --check`. Report verification accurately; do not claim proposed code has been compiled or tested when it has not been applied and checked. If validation requires non-formatting edits, explain the required changes and ask the user to apply them first.

Agents may create and maintain documentation, including plans, architecture notes, implementation guides, and review reports. Agents may update `AGENTS.md` when the user explicitly requests a policy change. Documentation permission does not authorize changes to code, tests, examples, build configuration, dependency files, or editor configuration. Code snippets in documentation are proposals for the user to apply, not permission to execute or install those changes. Explicitly requested formatting-only changes remain allowed under the preceding rule.

## Security & Agent Instructions

Never commit secrets or local environment files. Provide sanitized examples such as `.env.example` and pin dependencies with the ecosystem lockfile.

Agents must communicate in the language established by the current conversation and follow the repository language requirements and internal-documentation exception above. Treat this file as living policy: add durable constraints when they emerge, and periodically consolidate or remove obsolete guidance to keep it concise and authoritative.
