//! Plateau 12x12. Coordonnees : x = colonne (A = 0), y = 0 pour la ligne du haut
//! (affichee "12"). L'index lineaire `y * 12 + x` est l'ordre du plateau dans les
//! messages 1 et 2 du protocole.

use std::fmt;

use serde::{Deserialize, Serialize};

pub const SIZE: i8 = 12;
pub const CELLS: usize = 144;

pub const EMPTY: u8 = 0;
pub const DESTROYED: u8 = 1;
pub const BLACK: u8 = 3;
pub const RED: u8 = 5;

/// Leur ordre fixe celui dans lequel les coups sont generes.
pub const DIRS: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Color {
    Red,
    Black,
}

impl Color {
    pub fn other(self) -> Color {
        match self {
            Color::Red => Color::Black,
            Color::Black => Color::Red,
        }
    }

    /// Valeur du protocole : 0 rouge, 1 noir.
    pub fn index(self) -> usize {
        match self {
            Color::Red => 0,
            Color::Black => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Color::Red => "red",
            Color::Black => "black",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Square {
    pub x: i8,
    pub y: i8,
}

impl Square {
    pub const fn new(x: i8, y: i8) -> Square {
        Square { x, y }
    }

    pub fn offset(self, dx: i8, dy: i8) -> Option<Square> {
        let s = Square::new(self.x + dx, self.y + dy);
        s.on_board().then_some(s)
    }

    pub fn on_board(self) -> bool {
        (0..SIZE).contains(&self.x) && (0..SIZE).contains(&self.y)
    }

    pub fn index(self) -> usize {
        self.y as usize * SIZE as usize + self.x as usize
    }
}

impl fmt::Display for Square {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}{}", (b'A' + self.x as u8) as char, SIZE - self.y)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Move {
    pub from: Square,
    pub to: Square,
    pub destroyed: Square,
}

impl fmt::Display for Move {
    /// Format canonique compact, celui du JSONL.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}-{}-{}", self.from, self.to, self.destroyed)
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Board {
    cells: [u8; CELLS],
    red: Square,
    black: Square,
    /// Nombre de cases detruites : c'est le N des phases de destruction.
    destroyed: u32,
}

impl Default for Board {
    fn default() -> Board {
        Board::initial()
    }
}

impl Board {
    /// Rouge en G1, noir en F12.
    pub fn initial() -> Board {
        let mut cells = [EMPTY; CELLS];
        let red = Square::new(6, 11);
        let black = Square::new(5, 0);
        cells[red.index()] = RED;
        cells[black.index()] = BLACK;
        Board {
            cells,
            red,
            black,
            destroyed: 0,
        }
    }

    /// Construit un plateau depuis les 144 valeurs du protocole. Exige exactement un pion
    /// de chaque couleur et uniquement les valeurs 0, 1, 3, 5.
    pub fn from_cells(values: &[u8]) -> Result<Board, String> {
        if values.len() != CELLS {
            return Err(format!("{} cases au lieu de {CELLS}", values.len()));
        }
        let mut cells = [EMPTY; CELLS];
        let (mut red, mut black, mut destroyed) = (None, None, 0);
        for (i, &v) in values.iter().enumerate() {
            let sq = Square::new((i % 12) as i8, (i / 12) as i8);
            match v {
                EMPTY => {}
                DESTROYED => destroyed += 1,
                RED if red.is_none() => red = Some(sq),
                BLACK if black.is_none() => black = Some(sq),
                _ => return Err(format!("valeur {v} invalide ou pion en double en {sq}")),
            }
            cells[i] = v;
        }
        match (red, black) {
            (Some(red), Some(black)) => Ok(Board {
                cells,
                red,
                black,
                destroyed,
            }),
            _ => Err("pion rouge ou noir manquant".into()),
        }
    }

    pub fn cells(&self) -> &[u8; CELLS] {
        &self.cells
    }

    pub fn get(&self, sq: Square) -> u8 {
        self.cells[sq.index()]
    }

    pub fn is_empty(&self, sq: Square) -> bool {
        self.get(sq) == EMPTY
    }

    pub fn pawn(&self, c: Color) -> Square {
        match c {
            Color::Red => self.red,
            Color::Black => self.black,
        }
    }

    pub fn destroyed(&self) -> u32 {
        self.destroyed
    }

    /// Applique un coup suppose legal (voir `rules::is_legal`).
    pub fn apply(&mut self, m: Move) {
        let piece = self.cells[m.from.index()];
        debug_assert!(piece == RED || piece == BLACK);
        self.cells[m.from.index()] = EMPTY;
        self.cells[m.to.index()] = piece;
        self.cells[m.destroyed.index()] = DESTROYED;
        self.destroyed += 1;
        if piece == RED {
            self.red = m.to;
        } else {
            self.black = m.to;
        }
    }

    pub fn undo(&mut self, m: Move) {
        let piece = self.cells[m.to.index()];
        self.cells[m.destroyed.index()] = EMPTY;
        self.cells[m.to.index()] = EMPTY;
        self.cells[m.from.index()] = piece;
        self.destroyed -= 1;
        if piece == RED {
            self.red = m.from;
        } else {
            self.black = m.from;
        }
    }

    /// Plateau tel qu'envoye dans les messages 1 et 2 : 144 entiers separes par des espaces.
    pub fn to_protocol(&self) -> String {
        let mut s = String::with_capacity(CELLS * 2);
        for (i, v) in self.cells.iter().enumerate() {
            if i > 0 {
                s.push(' ');
            }
            s.push((b'0' + v) as char);
        }
        s
    }
}

impl fmt::Display for Board {
    /// Vue texte pour le debogage, ligne 12 en haut.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for y in 0..SIZE {
            write!(f, "{:>2} ", SIZE - y)?;
            for x in 0..SIZE {
                let c = match self.get(Square::new(x, y)) {
                    EMPTY => '.',
                    DESTROYED => '#',
                    RED => 'R',
                    BLACK => 'B',
                    _ => '?',
                };
                write!(f, "{c}")?;
            }
            writeln!(f)?;
        }
        write!(f, "   ABCDEFGHIJKL")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_positions_match_protocol_indices() {
        let b = Board::initial();
        assert_eq!(b.pawn(Color::Red).to_string(), "G1");
        assert_eq!(b.pawn(Color::Black).to_string(), "F12");
        assert_eq!(b.cells()[138], RED);
        assert_eq!(b.cells()[5], BLACK);
    }

    #[test]
    fn protocol_roundtrip() {
        let b = Board::initial();
        let values: Vec<u8> = b.to_protocol().split(' ').map(|v| v.parse().unwrap()).collect();
        assert_eq!(Board::from_cells(&values).unwrap(), b);
    }

    #[test]
    fn apply_then_undo_restores_board() {
        let mut b = Board::initial();
        let m = Move {
            from: Square::new(6, 11),
            to: Square::new(6, 8),
            destroyed: Square::new(5, 3),
        };
        b.apply(m);
        assert_eq!(b.pawn(Color::Red), Square::new(6, 8));
        assert_eq!(b.get(Square::new(5, 3)), DESTROYED);
        assert_eq!(b.destroyed(), 1);
        b.undo(m);
        assert_eq!(b, Board::initial());
    }

    #[test]
    fn from_cells_rejects_missing_pawn() {
        assert!(Board::from_cells(&[0; CELLS]).is_err());
    }
}
