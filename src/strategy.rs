//! Stratégies des bots de test, pour faire tourner le ladder sans jars d'équipes.

use rand::Rng;
use rand::seq::IndexedRandom;

use crate::board::{Board, Color, Move};
use crate::rules::{self, mobility};

#[derive(Clone, Copy, PartialEq, Eq, Debug, clap::ValueEnum)]
pub enum Strategy {
    /// Un coup légal au hasard.
    Random,
    /// Gagne tout de suite si possible, sinon maximise sa mobilité moins celle de l'adversaire ;
    /// hasard entre les ex aequo.
    Greedy,
}

impl Strategy {
    pub fn choose(self, board: &Board, color: Color, rng: &mut impl Rng) -> Option<Move> {
        let moves = rules::legal_moves(board, color);
        match self {
            Self::Random => moves.choose(rng).copied(),
            Self::Greedy => greedy(board, color, &moves, rng),
        }
    }
}

fn greedy(board: &Board, color: Color, moves: &[Move], rng: &mut impl Rng) -> Option<Move> {
    let mut board = board.clone();
    let mut best = Vec::new();
    let mut best_score = i32::MIN;
    for &mv in moves {
        board.apply(mv);
        let won = !rules::has_moves(&board, color.other());
        let score = mobility(&board, color) - mobility(&board, color.other());
        board.undo(mv);
        if won {
            return Some(mv);
        }
        if score > best_score {
            best_score = score;
            best.clear();
        }
        if score == best_score {
            best.push(mv);
        }
    }
    best.choose(rng).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::test_support::diagram;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn both_strategies_play_legal_moves() {
        let b = Board::initial();
        let mut rng = StdRng::seed_from_u64(1);
        for s in [Strategy::Random, Strategy::Greedy] {
            let m = s.choose(&b, Color::Red, &mut rng).unwrap();
            assert!(rules::is_legal(&b, Color::Red, m), "{s:?}");
        }
    }

    #[test]
    fn greedy_takes_immediate_win() {
        // Noir en A12 n'a qu'une sortie (B11) : rouge gagne en la détruisant.
        let b = diagram(
            "B#..........
             #...........
             ............
             ............
             ............
             ......R.....
             ############
             ############
             ############
             ............
             ............
             ............",
        );
        let mut rng = StdRng::seed_from_u64(1);
        let m = Strategy::Greedy.choose(&b, Color::Red, &mut rng).unwrap();
        let mut after = b.clone();
        after.apply(m);
        assert_eq!(rules::winner_after(&after, Color::Red), Some(Color::Red), "{m}");
    }

    #[test]
    fn greedy_beats_random() {
        let mut rng = StdRng::seed_from_u64(42);
        let mut greedy_wins = 0;
        for game in 0..20 {
            let greedy_color = if game % 2 == 0 { Color::Red } else { Color::Black };
            let mut b = Board::initial();
            let mut turn = Color::Red;
            let winner = loop {
                let s = if turn == greedy_color {
                    Strategy::Greedy
                } else {
                    Strategy::Random
                };
                let m = s
                    .choose(&b, turn, &mut rng)
                    .expect("le joueur au trait a toujours un coup");
                b.apply(m);
                if let Some(w) = rules::winner_after(&b, turn) {
                    break w;
                }
                turn = turn.other();
            };
            greedy_wins += u32::from(winner == greedy_color);
        }
        assert!(greedy_wins >= 16, "glouton : {greedy_wins}/20");
    }

    #[test]
    fn parses_names() {
        use clap::ValueEnum;
        assert_eq!(Strategy::from_str("greedy", false), Ok(Strategy::Greedy));
        assert!(Strategy::from_str("2", false).is_err());
    }
}
