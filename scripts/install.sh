#!/bin/sh
# Build this checkout and install one executable on the user's PATH.
set -eu
project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)

if ! command -v cargo >/dev/null 2>&1; then
    if [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    else
        echo "Installing stable Rust from rustup.rs..."
        installer=$(mktemp "${TMPDIR:-/tmp}/nano-codex-rust.XXXXXX")
        trap 'rm -f "$installer"' EXIT HUP INT TERM
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$installer"
        sh "$installer" -y --profile minimal --component rustfmt --component clippy --no-modify-path
        . "$HOME/.cargo/env"
    fi
fi

install_root=${NANO_CODEX_PREFIX:-"$HOME/.local"}
cargo install --path "$project_dir" --locked --root "$install_root" --force
echo "Installed $install_root/bin/nano-codex"
case ":$PATH:" in
    *":$install_root/bin:"*) ;;
    *) echo "Add this to your shell config: export PATH=\"$install_root/bin:\$PATH\"" ;;
esac
echo "Run nano-codex to choose an account or local inference server."
