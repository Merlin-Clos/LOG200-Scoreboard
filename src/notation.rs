//! Lecture et ecriture des coups. Le parseur reproduit la tolerance du serveur
//! officiel : ' ' et '-' sont des separateurs en nombre quelconque, y compris
//! zero (`G1E3C12`), et les lettres sont insensibles a la casse.

use crate::board::{Move, SIZE, Square};

/// Format envoye par le serveur officiel dans les messages 3 et 5.
pub fn to_protocol(m: Move) -> String {
    format!("{} - {} - {}", m.from, m.to, m.destroyed)
}

pub fn parse_square(s: &str) -> Option<Square> {
    let mut p = Parser::new(s.trim());
    let sq = p.square()?;
    p.skip_separators();
    p.at_end().then_some(sq)
}

/// Exige exactement trois cases. Ne verifie pas la legalite.
pub fn parse_move(s: &str) -> Option<Move> {
    let mut p = Parser::new(s.trim());
    let mut squares = [Square::new(0, 0); 3];
    for sq in &mut squares {
        p.skip_separators();
        *sq = p.square()?;
    }
    p.skip_separators();
    p.at_end().then_some(Move {
        from: squares[0],
        to: squares[1],
        destroyed: squares[2],
    })
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Parser<'a> {
        Parser {
            bytes: s.as_bytes(),
            pos: 0,
        }
    }

    fn at_end(&self) -> bool {
        self.pos == self.bytes.len()
    }

    fn skip_separators(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'-')) {
            self.pos += 1;
        }
    }

    fn square(&mut self) -> Option<Square> {
        let letter = self.bytes.get(self.pos)?.to_ascii_uppercase();
        if !(b'A'..b'A' + SIZE as u8).contains(&letter) {
            return None;
        }
        self.pos += 1;
        let start = self.pos;
        let mut row: u32 = 0;
        while let Some(d @ b'0'..=b'9') = self.bytes.get(self.pos) {
            row = row.saturating_mul(10).saturating_add((d - b'0') as u32);
            self.pos += 1;
        }
        if self.pos == start || row == 0 || row > SIZE as u32 {
            return None;
        }
        Some(Square::new((letter - b'A') as i8, SIZE - row as i8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(s: &str) -> String {
        parse_move(s).map(|m| m.to_string()).unwrap_or_default()
    }

    #[test]
    fn accepts_all_server_formats() {
        for s in ["G1-E3-C12", "G1E3C12", "G1 - E3 - C12", "g1 e3 c12", "  G1--E3 C12 "] {
            assert_eq!(mv(s), "G1-E3-C12", "{s:?}");
        }
    }

    #[test]
    fn rejects_malformed() {
        for s in [
            "",
            "G1-E3",
            "G1-E3-C12-D4",
            "G0-E3-C12",
            "G13-E3-C12",
            "M1-E3-C12",
            "G1;E3;C12",
            "G-E3-C12",
        ] {
            assert!(parse_move(s).is_none(), "{s:?}");
        }
    }

    #[test]
    fn coordinates_follow_server_convention() {
        let m = parse_move("A12-L1-F12").unwrap();
        assert_eq!(m.from, Square::new(0, 0));
        assert_eq!(m.to, Square::new(11, 11));
        assert_eq!(m.destroyed, Square::new(5, 0));
        assert_eq!(to_protocol(m), "A12 - L1 - F12");
    }
}
