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

## Building from source

```bash
make verify                 # format, check, Clippy, tests, release, packages
make dist                   # platform release archive
nix build .#default         # Nix package
docker build -t ekphos .    # Linux container
```

## Community

- Join the [nostacks Discord](https://discord.gg/R5dEQZfvb) for help, ideas, conversations, and project updates

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

[![Packaging status](https://repology.org/badge/vertical-allrepos/ekphos.svg)](https://repology.org/project/ekphos/versions)

## License

MIT
