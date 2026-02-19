#!/bin/bash
# Quick Translate - Build & Install
# Uso: ./install.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "Compilo..."
source "$HOME/.cargo/env"
cargo build --release

echo "Installo binari in ~/.local/bin/"
cp target/release/quick-translate ~/.local/bin/
cp target/release/quick-translate-popup ~/.local/bin/

echo "Fatto! Binari aggiornati."
