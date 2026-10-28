# iced_richedit

A rich text editor widget for [iced](https://iced.rs/), built on a ProseMirror-based document model.

> **Actively under development. Not ready for production use.**


## Dependencies

This project currently depends on iced `0.15.0-dev` and will not have a stable release until iced 0.15 is officially released.

## Try the example

```shell
cargo run --example basic --features example-basic
```

## Development

```shell
cargo check
cargo test
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```
