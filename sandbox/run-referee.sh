#!/usr/bin/env bash
# Lance l'arbitre d'une partie dans son bac a sable, sans reseau. Les variables SANDBOX_* viennent de
# la section [sandbox] de ladder.toml.
# Usage : run-referee.sh GAME PORT TIME_MS [OUVERTURE]
set -euo pipefail
game=$1 port=$2 time_ms=$3 opening=${4:-}
dir=$SANDBOX_RUNDIR/$game
mkdir -p "$dir/red" "$dir/black"
exec docker run --rm --name "$game-referee" --label isolation-ladder \
    --runtime "$SANDBOX_RUNTIME" --network none --read-only \
    -v "$dir/red:/chan/red" -v "$dir/black:/chan/black" \
    "$SANDBOX_IMAGE" referee-entry.sh "$port" \
    isolation-referee --port "$port" --time-ms "$time_ms" --opening "$opening" \
    --connect-timeout-ms "${SANDBOX_CONNECT_TIMEOUT_MS:-10000}"
