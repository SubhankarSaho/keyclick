#!/bin/bash
# One-shot macOS build: installs missing prerequisites, then produces release/KeyClick.dmg.
# Usage (in Terminal, inside the keyclick folder):  bash scripts/build-mac.sh
set -euo pipefail
cd "$(dirname "$0")/.."

if ! xcode-select -p >/dev/null 2>&1; then
  echo "Installing Apple command line tools (a system dialog opens). Re-run this script when it finishes."
  xcode-select --install
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Installing Rust..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
  source "$HOME/.cargo/env"
fi
rustup target add aarch64-apple-darwin x86_64-apple-darwin

if ! command -v node >/dev/null 2>&1; then
  echo "Node.js 20+ is required: install it from https://nodejs.org (LTS) and re-run this script."
  exit 1
fi

npm ci
npm test
npm run build-macos
echo
echo "Done: release/KeyClick.dmg"
open release
