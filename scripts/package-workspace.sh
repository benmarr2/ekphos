#!/bin/sh
set -eu

workspace=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
stage=$(mktemp -d "${TMPDIR:-/tmp}/ekphos-package.XXXXXX")
install_root=$(mktemp -d "${TMPDIR:-/tmp}/ekphos-install.XXXXXX")

cleanup() {
    rm -rf "$stage" "$install_root"
}
trap cleanup EXIT HUP INT TERM

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$workspace/Cargo.toml" | head -n 1)
[ -n "$version" ] || {
    printf 'unable to read the workspace version\n' >&2
    exit 1
}

packages="
ekphos-core
ekphos-canvas
ekphos-tasks
ekphos-editor
ekphos-graph
ekphos-bases
ekphos-search
ekphos-vault
ekphos-vim
ekphos-integrations
ekphos
"

cd "$workspace"

# Cargo's workspace verifier cannot consume sibling packages from its temporary
# registry offline. Package first, then compile the normalized manifests from
# their archives with every workspace crate patched to its staged copy.
cargo package --workspace --allow-dirty --locked --offline --no-verify

for crate in $packages; do
    archive="$workspace/target/package/$crate-$version.crate"
    [ -f "$archive" ] || {
        printf 'missing package: %s\n' "$archive" >&2
        exit 1
    }
    mkdir -p "$stage/$crate"
    tar -xzf "$archive" -C "$stage/$crate" --strip-components=1
done

env CARGO_TARGET_DIR="$workspace/target/package-install" cargo install \
    --path "$stage/ekphos" \
    --root "$install_root" \
    --locked \
    --offline \
    --config "patch.crates-io.ekphos-bases.path='$stage/ekphos-bases'" \
    --config "patch.crates-io.ekphos-canvas.path='$stage/ekphos-canvas'" \
    --config "patch.crates-io.ekphos-core.path='$stage/ekphos-core'" \
    --config "patch.crates-io.ekphos-editor.path='$stage/ekphos-editor'" \
    --config "patch.crates-io.ekphos-graph.path='$stage/ekphos-graph'" \
    --config "patch.crates-io.ekphos-integrations.path='$stage/ekphos-integrations'" \
    --config "patch.crates-io.ekphos-search.path='$stage/ekphos-search'" \
    --config "patch.crates-io.ekphos-tasks.path='$stage/ekphos-tasks'" \
    --config "patch.crates-io.ekphos-vault.path='$stage/ekphos-vault'" \
    --config "patch.crates-io.ekphos-vim.path='$stage/ekphos-vim'"

"$install_root/bin/ekphos" --version | grep -Fx "ekphos $version"
printf 'Packaged workspace installation passed.\n'
