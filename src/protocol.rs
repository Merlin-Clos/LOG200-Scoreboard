//! Protocole texte du serveur officiel, une commande par ligne (tableau dans docs/formats.md).

use std::io::{self, BufRead, BufReader, ErrorKind, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use crate::board::{Board, Color, Move};
use crate::notation;

pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Clone, Debug, PartialEq)]
pub enum ServerMsg {
    NewGame {
        rows: u32,
        cols: u32,
        time_ms: u64,
        color: Color,
    },
    /// Code 1 pour le rouge (vaut demande de coup), 2 pour le noir.
    Board {
        color: Color,
        board: Board,
    },
    /// `None` si le coup adverse est absent ou illisible (anciennes versions : `3 A8-A8`).
    Request(Option<Move>),
    Invalid,
    End {
        result: String,
        last: Option<Move>,
    },
}

impl ServerMsg {
    pub fn encode(&self) -> String {
        match self {
            ServerMsg::NewGame {
                rows,
                cols,
                time_ms,
                color,
            } => format!("0 {PROTOCOL_VERSION} {rows} {cols} {time_ms} {}", color.index()),
            ServerMsg::Board { color, board } => {
                format!("{} {}", color.index() + 1, board.to_protocol())
            }
            ServerMsg::Request(Some(m)) => format!("3 {}", notation::to_protocol(*m)),
            ServerMsg::Request(None) => "3".into(),
            ServerMsg::Invalid => "4".into(),
            ServerMsg::End { result, last } => match last {
                Some(m) => format!("5 {result} {}", notation::to_protocol(*m)),
                None => format!("5 {result}"),
            },
        }
    }

    pub fn decode(line: &str) -> Result<ServerMsg, String> {
        let line = line.trim();
        let (code, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        let ints = || -> Result<Vec<u64>, String> {
            rest.split_whitespace()
                .map(|t| t.parse().map_err(|_| format!("entier attendu : {t:?}")))
                .collect()
        };
        match code {
            "0" => match ints()?[..] {
                [_version, rows, cols, time_ms, color] => Ok(ServerMsg::NewGame {
                    rows: rows as u32,
                    cols: cols as u32,
                    time_ms,
                    color: if color == 0 { Color::Red } else { Color::Black },
                }),
                _ => Err(format!("message 0 mal forme : {line:?}")),
            },
            "1" | "2" => {
                let cells: Vec<u8> = ints()?.into_iter().map(|v| v as u8).collect();
                Ok(ServerMsg::Board {
                    color: if code == "1" { Color::Red } else { Color::Black },
                    board: Board::from_cells(&cells)?,
                })
            }
            "3" => Ok(ServerMsg::Request(notation::parse_move(rest))),
            "4" => Ok(ServerMsg::Invalid),
            "5" => {
                let (result, last) = rest.split_once(' ').unwrap_or((rest, ""));
                Ok(ServerMsg::End {
                    result: result.to_string(),
                    last: notation::parse_move(last),
                })
            }
            _ => Err(format!("commande inconnue : {line:?}")),
        }
    }
}

#[derive(Debug)]
pub enum RecvError {
    Timeout,
    Closed,
    Io(io::Error),
}

/// Connexion ligne par ligne avec echeance. Les octets d'une ligne incomplete sont conserves
/// d'un appel a l'autre.
pub struct LineConn {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    pending: Vec<u8>,
}

impl LineConn {
    pub fn new(stream: TcpStream) -> io::Result<LineConn> {
        stream.set_nodelay(true)?;
        Ok(LineConn {
            writer: stream.try_clone()?,
            reader: BufReader::new(stream),
            pending: Vec::new(),
        })
    }

    pub fn send(&mut self, line: &str) -> io::Result<()> {
        let mut buf = Vec::with_capacity(line.len() + 1);
        buf.extend_from_slice(line.as_bytes());
        buf.push(b'\n');
        self.writer.write_all(&buf)?;
        self.writer.flush()
    }

    /// Ligne suivante sans le `\n` final (ni `\r`). `None` comme echeance : attente illimitee.
    pub fn recv(&mut self, deadline: Option<Instant>) -> Result<String, RecvError> {
        loop {
            let timeout = match deadline {
                Some(d) => {
                    let left = d.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(RecvError::Timeout);
                    }
                    Some(left.max(Duration::from_millis(1)))
                }
                None => None,
            };
            self.reader.get_ref().set_read_timeout(timeout).map_err(RecvError::Io)?;
            match self.reader.read_until(b'\n', &mut self.pending) {
                Ok(0) => return Err(RecvError::Closed),
                Ok(_) if self.pending.ends_with(b"\n") => {
                    let line = String::from_utf8_lossy(&self.pending)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();
                    self.pending.clear();
                    return Ok(line);
                }
                Ok(_) => return Err(RecvError::Closed),
                Err(e)
                    if matches!(
                        e.kind(),
                        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
                    ) =>
                {
                    continue;
                }
                Err(e) if matches!(e.kind(), ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted) => {
                    return Err(RecvError::Closed);
                }
                Err(e) => return Err(RecvError::Io(e)),
            }
        }
    }

    pub fn shutdown(&self) {
        let _ = self.writer.shutdown(std::net::Shutdown::Both);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn encode_matches_server_strings() {
        let m = notation::parse_move("G1-E3-C12").unwrap();
        let new = ServerMsg::NewGame {
            rows: 12,
            cols: 12,
            time_ms: 5000,
            color: Color::Red,
        };
        assert_eq!(new.encode(), "0 2 12 12 5000 0");
        assert_eq!(ServerMsg::Request(Some(m)).encode(), "3 G1 - E3 - C12");
        let end = ServerMsg::End {
            result: "WIN".into(),
            last: Some(m),
        };
        assert_eq!(end.encode(), "5 WIN G1 - E3 - C12");
        let board = ServerMsg::Board {
            color: Color::Black,
            board: Board::initial(),
        };
        assert!(board.encode().starts_with("2 0 0 0 0 0 3 0"));
    }

    #[test]
    fn decode_roundtrip_and_legacy_first_request() {
        for msg in [
            ServerMsg::NewGame {
                rows: 12,
                cols: 12,
                time_ms: 500,
                color: Color::Black,
            },
            ServerMsg::Board {
                color: Color::Red,
                board: Board::initial(),
            },
            ServerMsg::Invalid,
            ServerMsg::End {
                result: "TIMEOUT".into(),
                last: None,
            },
        ] {
            assert_eq!(ServerMsg::decode(&msg.encode()).unwrap(), msg);
        }
        assert_eq!(ServerMsg::decode("3 A8-A8").unwrap(), ServerMsg::Request(None));
    }

    #[test]
    fn recv_keeps_partial_line_across_timeouts() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let writer = std::thread::spawn(move || {
            let mut s = TcpStream::connect(addr).unwrap();
            s.write_all(b"G1-").unwrap();
            std::thread::sleep(Duration::from_millis(80));
            s.write_all(b"E3-C12\r\nnext\n").unwrap();
            std::thread::sleep(Duration::from_millis(50));
        });
        let (stream, _) = listener.accept().unwrap();
        let mut conn = LineConn::new(stream).unwrap();
        let short = Instant::now() + Duration::from_millis(20);
        assert!(matches!(conn.recv(Some(short)), Err(RecvError::Timeout)));
        let long = Instant::now() + Duration::from_secs(2);
        assert_eq!(conn.recv(Some(long)).unwrap(), "G1-E3-C12");
        assert_eq!(conn.recv(Some(long)).unwrap(), "next");
        assert!(matches!(conn.recv(Some(long)), Err(RecvError::Closed)));
        writer.join().unwrap();
    }
}
