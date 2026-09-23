# Ekphos

[![Crates.io](https://img.shields.io/crates/v/ekphos)](https://crates.io/crates/ekphos)
[![Rust](https://img.shields.io/badge/rust-1.90%2B-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/crates/l/ekphos)](https://github.com/nostacks/ekphos/blob/main/LICENSE)

A lightweight, fast, terminal-based markdown research tool built with Rust.

![Ekphos Preview](examples/ekphos-screenshot.png)

## Documentation

**Go to [Documentation](https://ekphos.nostacks.xyz/docs)**

## Quick Start

To install with [Cargo](https://doc.rust-lang.org/cargo/):

```bash
cargo install ekphos
```

Alternatively, you can install Ekphos using [Homebrew](https://brew.sh):

```bash
brew install ekphos
```

Or using [AUR](https://aur.archlinux.org/packages/ekphos):

```bash
yay -S ekphos
```

_Note: Always update to the latest version. If you encounter config issues after updating, run `ekphos --reset` to reset your configuration._

## Requirements

- Rust 1.90+
- For inline images and graphical equations: iTerm2, Kitty, WezTerm, Ghostty, or a Sixel-compatible terminal

## Editing modes

Press F6 to choose Standard, Vim, or Helix when editing a Markdown note. The choice is saved for future sessions. You can also set it in your config file:

```toml
[editor]
mode = "helix"
```

Helix mode uses selections as cursors. Use `v` to extend selections, `C` to add a cursor below, `%` to select the note, and `s` to select regex matches within the current selections. Press `i` or `a` to insert, `Esc` to return to Normal mode, and `F1` for key help. In Helix mode, `:w` saves without leaving the editor, `:wq` saves and returns to preview, and `:q` returns to preview after checking for unsaved changes.

Helix shortcuts take priority while editing. For example, Ctrl+S saves a selection to the jump list; use `:w` to save the note. `Space f` opens the note picker and `Space /` opens content search after saving any edits. Features that need language servers, tree-sitter, split windows, or shell integration are not available in Ekphos.

## Pasting images

Paste an image while editing a note to save it in your vault and insert a Markdown link to it. Press Ctrl+V in Standard mode or in Vim and Helix Insert mode. From Vim Normal mode, use `"+p`. From Helix Normal mode, use `Space p`. You can also right-click and choose Paste in any mode. Screenshots and other image data are saved as `Pasted image <timestamp>.png`. Copied image files keep their names, and images already in your vault are linked where they are. If the clipboard also holds text, the text is pasted instead.

New images go to the `attachments` folder at the root of your vault. To change this, set `attachments_dir`:

```toml
[general]
attachments_dir = "./assets"
```

- `"attachments"` or any other relative path: a folder inside your vault
- `""`: the vault root
- `"./"`: the current note's folder
- `"./assets"`: a subfolder of the current note's folder

## Building from source

```bash
make verify                 # format, check, Clippy, tests, release, packages
make dist                   # platform release archive
nix build .#default         # Nix package
docker build -t ekphos .    # Linux container
```

## Community

- Join the [nostacks Discord](https://discord.gg/XBDstnqXVb) for help, ideas, conversations, and project updates

## Disclaimer

This project is in early development. There may be breaking changes and bugs in pre-releases.

## Contributing

```bash
git clone https://github.com/nostacks/ekphos.git
cd ekphos
```

1. Fork the repository
2. Create a feature branch from `main`
3. Make your changes
4. Submit a PR to the `main` branch

Read the [Ekphos documentation](https://ekphos.nostacks.xyz/docs).

## License

MIT
