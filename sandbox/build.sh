#!/usr/bin/env bash
# Construit l'image des bacs a sable a partir des binaires de ce depot.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --bin isolation-referee --bin isolation-bot
ctx=$(mktemp -d)
trap 'rm -rf "$ctx"' EXIT
cp target/release/isolation-referee target/release/isolation-bot sandbox/referee-entry.sh sandbox/player-entry.sh "$ctx/"
docker build -t "${SANDBOX_IMAGE:-isolation-sandbox}" -f sandbox/Dockerfile "$ctx"
