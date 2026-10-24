//! isolation-bot : adversaire de test pour le ladder et outil de conformance. Il verifie chaque coup
//! adverse et la fin de partie avec notre moteur. Avec `--probe-illegal`, il envoie d'abord a chaque
//! tour un coup que notre moteur refuse : si le serveur l'accepte, nos regles different des siennes.
//!
//! Usage : isolation-bot [--host 127.0.0.1] [--port 8888] [--strategy random|greedy] [--seed N]
//!                       [--probe-illegal] [--connect-timeout-ms 30000] [--verbose]

use std::collections::HashSet;
use std::net::TcpStream;
use std::time::{Duration, Instant};

use isolation::board::{Board, Color, DIRS, Move};
use isolation::cli::{Args, die};
use isolation::protocol::{LineConn, RecvError, ServerMsg};
use isolation::rules::{self, destinations};
use isolation::strategy::{self, Strategy};
use rand::SeedableRng;
use rand::rngs::StdRng;

const CATEGORIES: [&str; 3] = ["arrival", "distance", "jump"];

struct Game {
    board: Board,
    color: Color,
    /// Dernier coup envoye, en attente de verdict (`4` = refuse, `3`/`5` = accepte).
    pending: Option<Move>,
    pending_is_probe: Option<&'static str>,
    /// Dernier coup a nous accepte par le serveur.
    last_own: Option<Move>,
    probed_this_turn: bool,
    turn: usize,
}

#[derive(Default)]
struct Report {
    games: u32,
    checked: u32,
    mismatches: Vec<String>,
    probes_rejected: u32,
    probes_accepted: Vec<String>,
}

fn main() {
    let args = Args::from_env();
    let host = args.get("--host").unwrap_or("127.0.0.1").to_string();
    let port: u16 = args.parse("--port", 8888);
    let strategy: Strategy = args
        .get("--strategy")
        .unwrap_or("greedy")
        .parse()
        .unwrap_or_else(|e: String| die(&e));
    let mut rng = match args.get("--seed") {
        Some(s) => StdRng::seed_from_u64(s.parse().unwrap_or_else(|_| die("--seed invalide"))),
        None => StdRng::from_os_rng(),
    };
    let probe = args.flag("--probe-illegal");
    let verbose = args.flag("--verbose");
    let connect_timeout = Duration::from_millis(args.parse("--connect-timeout-ms", 30_000));

    let mut conn = connect(&host, port, connect_timeout);
    let mut report = Report::default();
    let mut game: Option<Game> = None;

    loop {
        let line = match conn.recv(None) {
            Ok(l) => l,
            Err(RecvError::Closed) => break,
            Err(e) => die(&format!("lecture : {e:?}")),
        };
        if verbose {
            eprintln!("<- {}", abbreviate(&line));
        }
        let msg = match ServerMsg::decode(&line) {
            Ok(m) => m,
            Err(e) => {
                report.mismatches.push(format!("message illisible : {e}"));
                continue;
            }
        };
        match msg {
            ServerMsg::NewGame {
                rows,
                cols,
                time_ms,
                color,
            } => {
                if (rows, cols) != (12, 12) {
                    die(&format!("plateau {rows}x{cols} non gere"));
                }
                eprintln!("nouvelle partie : {} , {time_ms} ms", color.name());
                game = Some(Game {
                    board: Board::initial(),
                    color,
                    pending: None,
                    pending_is_probe: None,
                    last_own: None,
                    probed_this_turn: false,
                    turn: 0,
                });
            }
            ServerMsg::Board { color, board } => {
                let g = game.as_mut().unwrap_or_else(|| die("plateau recu avant le message 0"));
                g.board = board;
                if color == Color::Red {
                    g.new_turn();
                    g.play(&mut conn, strategy, &mut rng, probe, verbose, &mut report);
                }
            }
            ServerMsg::Request(opp) => {
                let g = game.as_mut().unwrap_or_else(|| die("demande de coup hors partie"));
                g.settle(&mut report);
                if let Some(m) = opp {
                    g.opponent_move(m, &mut report);
                }
                g.new_turn();
                g.play(&mut conn, strategy, &mut rng, probe, verbose, &mut report);
            }
            ServerMsg::Invalid => {
                let g = game.as_mut().unwrap_or_else(|| die("4 hors partie"));
                match (g.pending.take(), g.pending_is_probe.take()) {
                    (Some(_), Some(_)) => report.probes_rejected += 1,
                    (m, _) => report
                        .mismatches
                        .push(format!("notre coup {m:?} refuse par le serveur")),
                }
                g.play(&mut conn, strategy, &mut rng, probe, verbose, &mut report);
            }
            ServerMsg::End { result, last } => {
                let Some(mut g) = game.take() else {
                    die("fin de partie hors partie");
                };
                g.settle(&mut report);
                if let Some(m) = last
                    && Some(m) != g.last_own
                {
                    g.opponent_move(m, &mut report);
                }
                g.check_end(&result, &mut report);
                report.games += 1;
                eprintln!("fin : {result} ({} coups detruits)", g.board.destroyed());
            }
        }
    }

    eprintln!(
        "bilan : {} partie(s), {} coups adverses verifies, {} divergence(s), sondes refusees {} / acceptees {}",
        report.games,
        report.checked,
        report.mismatches.len(),
        report.probes_rejected,
        report.probes_accepted.len()
    );
    for m in report.mismatches.iter().chain(&report.probes_accepted) {
        eprintln!("  DIVERGENCE {m}");
    }
    if !report.mismatches.is_empty() || !report.probes_accepted.is_empty() {
        std::process::exit(1);
    }
}

impl Game {
    /// Un `3` ou un `5` confirme que notre dernier coup a ete accepte.
    fn settle(&mut self, report: &mut Report) {
        if let Some(m) = self.pending.take() {
            if let Some(cat) = self.pending_is_probe.take() {
                report
                    .probes_accepted
                    .push(format!("sonde {cat} acceptee : {m}\n{}", self.board));
            }
            self.board.apply(m);
            self.last_own = Some(m);
        }
    }

    fn new_turn(&mut self) {
        self.turn += 1;
        self.probed_this_turn = false;
    }

    fn opponent_move(&mut self, m: Move, report: &mut Report) {
        let opp = self.color.other();
        report.checked += 1;
        if !rules::is_legal(&self.board, opp, m) {
            report
                .mismatches
                .push(format!("coup adverse juge illegal : {m}\n{}", self.board));
        }
        self.board.apply(m);
    }

    fn check_end(&self, result: &str, report: &mut Report) {
        let expected = match result {
            "WIN" => !rules::has_moves(&self.board, self.color.other()),
            "LOSS" => !rules::has_moves(&self.board, self.color),
            "TIMEOUT" => true,
            other => {
                report.mismatches.push(format!("resultat inattendu : {other}"));
                return;
            }
        };
        if !expected {
            report
                .mismatches
                .push(format!("fin {result} non confirmee par notre moteur\n{}", self.board));
        }
    }

    /// Envoie une sonde (une fois par tour) ou notre coup. Un coup reel refuse est renvoye a
    /// l'identique : c'est une divergence, et le serveur finira par declarer un timeout.
    fn play(
        &mut self,
        conn: &mut LineConn,
        strategy: Strategy,
        rng: &mut StdRng,
        probe: bool,
        verbose: bool,
        report: &mut Report,
    ) {
        if probe && !self.probed_this_turn {
            self.probed_this_turn = true;
            if let Some((m, cat)) = pick_probe(&self.board, self.color, self.turn) {
                self.send(conn, m, verbose);
                self.pending_is_probe = Some(cat);
                return;
            }
        }
        let Some(m) = strategy::choose(strategy, &self.board, self.color, rng) else {
            report
                .mismatches
                .push(format!("demande de coup sans coup legal\n{}", self.board));
            return;
        };
        self.send(conn, m, verbose);
    }

    fn send(&mut self, conn: &mut LineConn, m: Move, verbose: bool) {
        let text = m.to_string();
        if verbose {
            eprintln!("-> {text}");
        }
        conn.send(&text).unwrap_or_else(|e| die(&format!("envoi : {e}")));
        self.pending = Some(m);
    }
}

/// Coups que notre moteur refuse, classes par regle testee :
/// - `arrival` : detruire sa propre case d'arrivee
/// - `distance` : cible a une distance du pion adverse que la phase (ou le repli) n'autorise pas
/// - `jump` : passer par-dessus le pion adverse
fn pick_probe(b: &Board, c: Color, turn: usize) -> Option<(Move, &'static str)> {
    let legal: HashSet<Move> = rules::legal_moves(b, c).into_iter().collect();
    let from = b.pawn(c);
    let opp = b.pawn(c.other());
    let mut by_cat: [Vec<Move>; 3] = Default::default();
    destinations(b, from, |to| {
        by_cat[0].push(Move {
            from,
            to,
            destroyed: to,
        });
        for (dx, dy) in DIRS {
            for k in 1..=3 {
                if let Some(t) = opp.offset(dx * k, dy * k)
                    && t != to
                    && (t == from || b.is_empty(t))
                {
                    by_cat[1].push(Move { from, to, destroyed: t });
                }
            }
        }
    });
    let target = legal.iter().next().map(|m| m.destroyed);
    for (dx, dy) in DIRS {
        let mut cur = from;
        let mut jumped = false;
        for _ in 0..3 {
            let Some(next) = cur.offset(dx, dy) else { break };
            cur = next;
            if next == opp {
                jumped = true;
                continue;
            }
            if !b.is_empty(next) {
                break;
            }
            if jumped && let Some(t) = target.filter(|&t| t != next) {
                by_cat[2].push(Move {
                    from,
                    to: next,
                    destroyed: t,
                });
            }
        }
    }
    for cat in &mut by_cat {
        cat.retain(|m| !legal.contains(m));
    }
    (0..CATEGORIES.len())
        .map(|i| (turn + i) % CATEGORIES.len())
        .find(|&i| !by_cat[i].is_empty())
        .map(|i| (by_cat[i][turn % by_cat[i].len()], CATEGORIES[i]))
}

fn connect(host: &str, port: u16, timeout: Duration) -> LineConn {
    let start = Instant::now();
    loop {
        match TcpStream::connect((host, port)) {
            Ok(s) => return LineConn::new(s).unwrap_or_else(|e| die(&format!("socket : {e}"))),
            Err(e) if start.elapsed() > timeout => die(&format!("connexion a {host}:{port} : {e}")),
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn abbreviate(line: &str) -> String {
    if line.len() > 60 {
        format!("{}...", &line[..60])
    } else {
        line.to_string()
    }
}
