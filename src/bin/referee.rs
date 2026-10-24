//! isolation-referee : arbitre d'une partie, avec le protocole du serveur officiel.
//! Le contrat (marqueurs sur stderr, JSON sur stdout) est dans docs/formats.md.
//!
//! Usage : isolation-referee --port P --time-ms T [--opening "G1-G3-F9,F12-F10-G7"]
//!                           [--max-invalid N] [--connect-timeout-ms 30000] [--bind 127.0.0.1]

use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use isolation::board::{Board, Color, Move};
use isolation::cli::{Args, die};
use isolation::notation;
use isolation::protocol::{LineConn, RecvError, ServerMsg};
use isolation::record::{GameResult, InvalidCounts, Reason, VERSION};
use isolation::rules;

fn main() {
    let args = Args::from_env();
    let port: u16 = args
        .require("--port")
        .parse()
        .unwrap_or_else(|_| die("--port invalide"));
    let time_ms: u64 = args
        .require("--time-ms")
        .parse()
        .unwrap_or_else(|_| die("--time-ms invalide"));
    let max_invalid: u32 = args.parse("--max-invalid", u32::MAX);
    let connect_timeout = Duration::from_millis(args.parse("--connect-timeout-ms", 30_000));
    let bind = args.get("--bind").unwrap_or("127.0.0.1");
    let opening: Vec<String> = args
        .get("--opening")
        .map(|s| {
            s.split(',')
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let board = match rules::replay(opening.iter().map(String::as_str)) {
        Ok((_, _, Some(_))) => die("l'ouverture termine la partie"),
        Ok((_, Color::Black, _)) => die("l'ouverture doit compter un nombre pair de demi-coups"),
        Ok((b, Color::Red, None)) => b,
        Err((i, m)) => die(&format!("coup d'ouverture {i} illegal : {m}")),
    };
    // Format canonique dans le resultat, quel que soit le format recu.
    let opening: Vec<String> = opening
        .iter()
        .map(|m| notation::parse_move(m).unwrap().to_string())
        .collect();

    let listener = TcpListener::bind((bind, port)).unwrap_or_else(|e| die(&format!("ecoute sur {bind}:{port} : {e}")));
    eprintln!("LISTENING {port}");

    let start = Instant::now();
    let mut players: Vec<LineConn> = Vec::with_capacity(2);
    let mut absent = None;
    for color in [Color::Red, Color::Black] {
        // Un delai par joueur : le ladder ne lance le noir qu'apres la connexion du rouge.
        let Some(stream) = accept(&listener, Instant::now() + connect_timeout) else {
            eprintln!("{} ne s'est pas connecte", color.name());
            absent = Some(color);
            break;
        };
        let mut conn = LineConn::new(stream).unwrap_or_else(|e| die(&format!("socket : {e}")));
        eprintln!("CONNECTED {}", color.name());
        let mut hello = vec![ServerMsg::NewGame {
            rows: 12,
            cols: 12,
            time_ms,
            color,
        }];
        if color == Color::Black {
            hello.push(ServerMsg::Board {
                color,
                board: board.clone(),
            });
        }
        // Une erreur d'envoi ici sera revue comme deconnexion au premier tour du joueur.
        for msg in hello {
            let _ = conn.send(&msg.encode());
        }
        players.push(conn);
    }

    let mut game = Referee {
        board,
        players,
        time: Duration::from_millis(time_ms),
        max_invalid,
        moves: Vec::new(),
        invalid: InvalidCounts::default(),
    };
    let (winner, reason) = match absent {
        // Le joueur absent perd sans qu'aucun coup soit joue ; le rouge, s'il est la, est prevenu.
        Some(color) => {
            if color == Color::Black {
                game.send(
                    Color::Red,
                    &ServerMsg::End {
                        result: "WIN".into(),
                        last: None,
                    },
                );
            }
            (color.other(), Reason::NoShow)
        }
        None => game.run(),
    };
    for p in &game.players {
        p.shutdown();
    }
    let result = GameResult {
        v: VERSION,
        time_ms,
        opening,
        moves: game.moves.iter().map(Move::to_string).collect(),
        winner,
        reason,
        invalid: game.invalid,
        duration_ms: start.elapsed().as_millis() as u64,
    };
    eprintln!(
        "gagnant {} ({:?}) en {} coups",
        winner.name(),
        reason,
        result.moves.len()
    );
    println!("{}", serde_json::to_string(&result).expect("serialisation"));
}

struct Referee {
    board: Board,
    players: Vec<LineConn>,
    time: Duration,
    max_invalid: u32,
    moves: Vec<Move>,
    invalid: InvalidCounts,
}

impl Referee {
    fn send(&mut self, c: Color, msg: &ServerMsg) -> bool {
        self.players[c.index()].send(&msg.encode()).is_ok()
    }

    fn run(&mut self) -> (Color, Reason) {
        let mut turn = Color::Red;
        loop {
            let request = match self.moves.last() {
                None => ServerMsg::Board {
                    color: Color::Red,
                    board: self.board.clone(),
                },
                Some(&m) => ServerMsg::Request(Some(m)),
            };
            if !self.send(turn, &request) {
                return self.forfeit(turn, Reason::Disconnect);
            }
            let deadline = Instant::now() + self.time;
            let m = loop {
                match self.players[turn.index()].recv(Some(deadline)) {
                    Err(RecvError::Timeout) => return self.forfeit(turn, Reason::Timeout),
                    Err(_) => return self.forfeit(turn, Reason::Disconnect),
                    Ok(line) => match notation::parse_move(&line).filter(|&m| rules::is_legal(&self.board, turn, m)) {
                        Some(m) => break m,
                        None => {
                            self.invalid.add(turn);
                            eprintln!("coup invalide de {} : {line:?}", turn.name());
                            let count = match turn {
                                Color::Red => self.invalid.red,
                                Color::Black => self.invalid.black,
                            };
                            if count >= self.max_invalid {
                                return self.forfeit(turn, Reason::Protocol);
                            }
                            if !self.send(turn, &ServerMsg::Invalid) {
                                return self.forfeit(turn, Reason::Disconnect);
                            }
                        }
                    },
                }
            };
            self.board.apply(m);
            self.moves.push(m);
            if let Some(winner) = rules::winner_after(&self.board, turn) {
                for (c, result) in [(winner, "WIN"), (winner.other(), "LOSS")] {
                    self.send(
                        c,
                        &ServerMsg::End {
                            result: result.into(),
                            last: Some(m),
                        },
                    );
                }
                return (winner, Reason::NoMoves);
            }
            turn = turn.other();
        }
    }

    /// `loser` perd sans avoir joue. Comme le serveur officiel, on annonce un timeout par `5 TIMEOUT`
    /// aux deux joueurs. Les autres cas recoivent WIN ou LOSS, sans coup.
    fn forfeit(&mut self, loser: Color, reason: Reason) -> (Color, Reason) {
        let winner = loser.other();
        for (c, result) in [(winner, "WIN"), (loser, "LOSS")] {
            let result = if reason == Reason::Timeout { "TIMEOUT" } else { result };
            self.send(
                c,
                &ServerMsg::End {
                    result: result.into(),
                    last: None,
                },
            );
        }
        (winner, reason)
    }
}

fn accept(listener: &TcpListener, deadline: Instant) -> Option<TcpStream> {
    listener.set_nonblocking(true).ok()?;
    loop {
        match listener.accept() {
            Ok((s, _)) => {
                s.set_nonblocking(false).ok()?;
                return Some(s);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() > deadline {
                    return None;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(_) => return None,
        }
    }
}
