# LOG200-Scoreboard

Live leaderboard (Elo, game history, replays) for the Isolation lab of LOG200. Design notes and open questions are in [`CLAUDE.md`](CLAUDE.md).

## Getting started

1. Clone this repository next to [LOG200-Isolation](https://github.com/VianAtETS/LOG200-Isolation): the dev container builds the game server image and mounts its binary from `../LOG200-Isolation`.
   ```text
   ÉTS/
   ├── LOG200-Isolation/
   └── LOG200-Scoreboard/
   ```
2. Open the folder in VS Code and choose Reopen in Container. Docker Compose starts two containers:
   - `dev`: JDK 25, Node 24, Rust, Claude Code, Ghidra 12.1.4 (headless) and the [REA](https://github.com/morluto/rea) MCP server and skill. VS Code attaches to this one.
   - `server`: the official game server under Wine, shown in a VS Code tab (noVNC on port 6080) and listening on `localhost:8888`.
3. Run `claude` once to log in. The login and the REA registration live in the `claude-config` Docker volume and survive container rebuilds.
4. The server binary is mounted read-only at `/opt/boardgame/BoardGame.exe` in `dev`. Ask Claude to analyze it with REA; `npx -y rea-agents@6.7.0 doctor` checks that the Ghidra provider is healthy.

## Components

| Path | Role |
| --- | --- |
| `src/rules.rs` | Game rules as applied by the official server (see `docs/rules.md`) |
| `isolation-referee` | Headless referee for one game; speaks the official protocol, prints one JSON result |
| `isolation-bot` | Protocol client with a simple strategy (random or greedy), used for tests and conformance checks |
| `isolation-ladder` | Schedules games between teams, appends to `data/games.jsonl` |
| `isolation-check` | Replays and validates a JSONL file or an official save (`BOARDGAME 1`) |
| `web/` | Static page (vanilla JS): Elo ranking, recent games, replay |

The referee contract and the JSONL format are described in `docs/formats.md`. Any referee that follows it, such as the instructor's C++ CLI, can replace ours.

```sh
cargo build --release
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

## Reverse engineering

This repository is public and the server belongs to the course instructor. Never commit anything derived from the binary (notes with addresses, Ghidra projects, decompiled code, dumps): keep it under `re/`, which git ignores. `docs/rules.md` and `src/rules.rs` only describe what the server does, as seen from a client.
