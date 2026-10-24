# LOG200-Scoreboard

Live leaderboard (Elo, game history, replays) for the Isolation lab of LOG200.

## Getting started

1. Optional, for conformance checks: copy the official game server from the course (`BoardGame.exe` and its DLLs and folders) into `official-server/`. It belongs to the instructor and is never committed (the folder is git-ignored). Without it, everything else still works.
2. Open the folder in VS Code and choose Reopen in Container. Docker Compose starts two containers:
   - `dev`: Rust, JDK 25, Node 24, and Docker with gVisor for the player sandboxes. VS Code attaches to this one.
   - `server`: the official game server under Wine, shown in a VS Code tab (noVNC on port 6080) and listening on `localhost:8888`, if `official-server/` contains it.

## Components

| Path | Role |
| --- | --- |
| `src/rules.rs` | Game rules as applied by the official server (see `docs/rules.md`) |
| `isolation-referee` | Headless referee for one game; speaks the official protocol, prints one JSON result |
| `isolation-bot` | Protocol client with a simple strategy (random or greedy), used for tests and conformance checks |
| `isolation-ladder` | Schedules games between teams, appends to `data/games.jsonl` |
| `isolation-check` | Replays and validates a JSONL file or an official save (`BOARDGAME 1`) |
| `sandbox/` | gVisor sandboxes for the referee and each player (see `docs/formats.md`) |
| `web/` | Static page (vanilla JS): Elo ranking, recent games, replay |

The referee contract and the JSONL format are described in `docs/formats.md`. Any referee that follows it, such as the instructor's C++ CLI, can replace ours.

```sh
cargo build --release
sandbox/build.sh                     # sandbox image (needs Docker with gVisor)
cp ladder.example.toml ladder.toml   # teams, time per move, session
target/release/isolation-ladder ladder.toml
npx http-server . -p 8000            # then open http://localhost:8000/web/
```

Tests: `cargo test` and `node --test web/*.test.mjs`.

## Conformance against the official server

Start a game in the server GUI (noVNC, port 6080) with one socket player and one CPU player, then connect our bot as the socket player:

```sh
target/release/isolation-bot --port 8888 --strategy random --probe-illegal
```

The bot checks every move of the official CPU with our engine. Each turn it first sends a move our engine considers illegal and expects `4` back. A `DIVERGENCE` line in its report means our rules differ from the server's.
