//! Regles du serveur officiel, detaillees dans docs/rules.md. Le point facile a rater : le repli de
//! distance (3, puis 2, puis 1) porte sur l'ensemble des coups possibles, pas sur chaque deplacement.

use crate::board::{Board, Color, DIRS, Move, Square};

pub fn destinations(b: &Board, from: Square, mut f: impl FnMut(Square)) {
    for (dx, dy) in DIRS {
        let mut cur = from;
        for _ in 0..3 {
            match cur.offset(dx, dy) {
                Some(next) if b.is_empty(next) => {
                    cur = next;
                    f(cur);
                }
                _ => break,
            }
        }
    }
}

/// Nombre de cases atteignables, sans tenir compte de la contrainte de destruction.
pub fn mobility(b: &Board, c: Color) -> i32 {
    let mut n = 0;
    destinations(b, b.pawn(c), |_| n += 1);
    n
}

fn generate_at(b: &Board, from: Square, opp: Square, kmin: i8, kmax: i8, out: &mut Vec<Move>) {
    destinations(b, from, |to| {
        for (dx, dy) in DIRS {
            for k in kmin..=kmax {
                let Some(t) = opp.offset(dx * k, dy * k) else {
                    break;
                };
                if t != to && (t == from || b.is_empty(t)) {
                    out.push(Move { from, to, destroyed: t });
                }
            }
        }
    });
}

/// Remplace le contenu de `out` par les coups legaux de `c`.
pub fn generate(b: &Board, c: Color, out: &mut Vec<Move>) {
    out.clear();
    let from = b.pawn(c);
    let opp = b.pawn(c.other());
    let n = b.destroyed();
    if n < 10 {
        for k in [3, 2, 1] {
            generate_at(b, from, opp, k, k, out);
            if !out.is_empty() {
                return;
            }
        }
    } else if n < 20 {
        for k in [2, 1] {
            generate_at(b, from, opp, k, k, out);
            if !out.is_empty() {
                return;
            }
        }
    } else {
        generate_at(b, from, opp, 1, 2, out);
    }
}

pub fn legal_moves(b: &Board, c: Color) -> Vec<Move> {
    let mut out = Vec::new();
    generate(b, c, &mut out);
    out
}

pub fn is_legal(b: &Board, c: Color, m: Move) -> bool {
    m.from == b.pawn(c) && legal_moves(b, c).contains(&m)
}

pub fn has_moves(b: &Board, c: Color) -> bool {
    !legal_moves(b, c).is_empty()
}

/// Apres un coup de `mover` : gagnant si l'adversaire n'a plus aucun coup complet.
/// Il n'y a pas de partie nulle en Isolation.
pub fn winner_after(b: &Board, mover: Color) -> Option<Color> {
    (!has_moves(b, mover.other())).then_some(mover)
}

/// Rejoue une suite de coups depuis le plateau initial, rouge en premier. Retourne le plateau
/// final, la couleur au trait et le gagnant si la partie s'est terminee sur le dernier coup.
/// Erreur : index et texte du premier coup illisible, illegal ou joue apres la fin.
pub fn replay<'a>(moves: impl IntoIterator<Item = &'a str>) -> Result<(Board, Color, Option<Color>), (usize, String)> {
    let mut b = Board::initial();
    let mut turn = Color::Red;
    let mut winner = None;
    for (i, text) in moves.into_iter().enumerate() {
        let m = crate::notation::parse_move(text)
            .filter(|&m| winner.is_none() && is_legal(&b, turn, m))
            .ok_or((i, text.to_string()))?;
        b.apply(m);
        winner = winner_after(&b, turn);
        turn = turn.other();
    }
    Ok((b, turn, winner))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::board::{BLACK, DESTROYED, EMPTY, RED};
    use crate::notation::parse_square;

    /// Plateau depuis un schema de 12 lignes (ligne 12 en premier) : `.` vide, `#` detruite,
    /// `R` rouge, `B` noir. Les espaces sont ignores.
    pub fn diagram(s: &str) -> Board {
        let cells: Vec<u8> = s
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '.' => EMPTY,
                '#' => DESTROYED,
                'R' => RED,
                'B' => BLACK,
                _ => panic!("caractere {c:?}"),
            })
            .collect();
        Board::from_cells(&cells).unwrap()
    }

    fn sq(s: &str) -> Square {
        parse_square(s).unwrap()
    }

    fn chebyshev_ray_distance(a: Square, b: Square) -> Option<i8> {
        let (dx, dy) = ((b.x - a.x).abs(), (b.y - a.y).abs());
        (dx == 0 || dy == 0 || dx == dy).then_some(dx.max(dy))
    }

    #[test]
    fn initial_red_moves_destroy_at_distance_three_from_black() {
        let b = Board::initial();
        let moves = legal_moves(&b, Color::Red);
        assert!(!moves.is_empty());
        let black = b.pawn(Color::Black);
        for m in &moves {
            assert_eq!(chebyshev_ray_distance(black, m.destroyed), Some(3), "{m}");
        }
        // F12 -> rayons a distance 3 : C12, I12, F9, C9, I9.
        let mut targets: Vec<String> = moves.iter().map(|m| m.destroyed.to_string()).collect();
        targets.sort();
        targets.dedup();
        assert_eq!(targets, ["C12", "C9", "F9", "I12", "I9"]);
    }

    #[test]
    fn opponent_pawn_blocks_movement() {
        let b = diagram(
            "............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             RB..........",
        );
        let moves = legal_moves(&b, Color::Red);
        assert!(moves.iter().all(|m| m.to.y != 11), "deplacement horizontal a travers B");
    }

    #[test]
    fn fallback_is_global_not_per_move() {
        // Noir en A12 : a distance 3 il ne reste que D9 (D12 et A9 sont detruites).
        // Rouge peut atterrir en D9. Ce coup-la n'a aucune cible a 3, mais d'autres en ont, donc il
        // est interdit.
        let b = diagram(
            "B..#........
             ............
             ............
             #...........
             ............
             ............
             ...R........
             ............
             ............
             ............
             ............
             ............",
        );
        let moves = legal_moves(&b, Color::Red);
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.destroyed == sq("D9")));
        assert!(moves.iter().all(|m| m.to != sq("D9")));
    }

    #[test]
    fn fallback_to_distance_two_when_no_distance_three_target() {
        let b = diagram(
            "B..#........
             ............
             ............
             #..#........
             ............
             ............
             ...R........
             ............
             ............
             ............
             ............
             ............",
        );
        let black = b.pawn(Color::Black);
        let moves = legal_moves(&b, Color::Red);
        assert!(!moves.is_empty());
        for m in &moves {
            assert_eq!(chebyshev_ray_distance(black, m.destroyed), Some(2), "{m}");
        }
    }

    #[test]
    fn origin_square_is_destroyable_arrival_is_not() {
        // Seule cible restante a distance 3 de A12 : D9, occupee par le rouge.
        let b = diagram(
            "B..#........
             ............
             ............
             #..R........
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............",
        );
        let moves = legal_moves(&b, Color::Red);
        assert!(!moves.is_empty());
        assert!(moves.iter().all(|m| m.destroyed == m.from));
    }

    #[test]
    fn middle_phase_uses_distance_two() {
        // 10 cases detruites loin des pions.
        let b = diagram(
            "............
             ............
             ............
             ......B.....
             ............
             ............
             ............
             ............
             ............
             ...R........
             ##########..
             ............",
        );
        assert_eq!(b.destroyed(), 10);
        let black = b.pawn(Color::Black);
        for m in legal_moves(&b, Color::Red) {
            assert_eq!(chebyshev_ray_distance(black, m.destroyed), Some(2), "{m}");
        }
    }

    #[test]
    fn final_phase_allows_distance_one_and_two() {
        let b = diagram(
            "............
             ............
             ............
             ......B.....
             ............
             ............
             ............
             ............
             ..........##
             ...R......##
             ############
             ....####....",
        );
        assert_eq!(b.destroyed(), 20);
        let black = b.pawn(Color::Black);
        let moves = legal_moves(&b, Color::Red);
        let mut dists: Vec<i8> = moves
            .iter()
            .map(|m| chebyshev_ray_distance(black, m.destroyed).unwrap())
            .collect();
        dists.sort();
        dists.dedup();
        assert_eq!(dists, [1, 2]);
    }

    #[test]
    fn blocked_player_has_no_moves_and_loses() {
        let b = diagram(
            "R#..........
             ##..........
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ............
             ...........B",
        );
        assert!(!has_moves(&b, Color::Red));
        assert_eq!(winner_after(&b, Color::Black), Some(Color::Black));
        assert_eq!(winner_after(&b, Color::Red), None);
    }

    #[test]
    fn replay_rejects_illegal_and_reports_index() {
        let b = Board::initial();
        let first = legal_moves(&b, Color::Red)[0].to_string();
        let (after, turn, winner) = replay([first.as_str()]).unwrap();
        assert_eq!(after.destroyed(), 1);
        assert_eq!(turn, Color::Black);
        assert_eq!(winner, None);
        // Rouge rejoue : le pion a bouger n'est plus le sien.
        assert_eq!(replay([first.as_str(), first.as_str()]).unwrap_err().0, 1);
    }

    #[test]
    fn is_legal_checks_owner_and_membership() {
        let b = Board::initial();
        let m = legal_moves(&b, Color::Red)[0];
        assert!(is_legal(&b, Color::Red, m));
        assert!(!is_legal(&b, Color::Black, m));
        let bad = Move { destroyed: m.to, ..m };
        assert!(!is_legal(&b, Color::Red, bad));
    }
}
