//! Bots de test, pour faire tourner le ladder sans jars d'equipes. `random` joue un coup legal au
//! hasard. `greedy` gagne tout de suite s'il le peut, sinon maximise sa mobilite moins celle de
//! l'adversaire apres le coup, et tire au hasard entre les ex aequo.

use std::str::FromStr;

use rand::Rng;
use rand::seq::IndexedRandom;

use crate::board::{Board, Color, Move};
use crate::rules::{self, mobility};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Strategy {
    Random,
    Greedy,
}

impl FromStr for Strategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Strategy, String> {
        match s {
            "random" => Ok(Strategy::Random),
            "greedy" => Ok(Strategy::Greedy),
            _ => Err(format!("strategie inconnue : {s} (random ou greedy)")),
        }
    }
}

/// `None` si `c` n'a aucun coup legal.
pub fn choose(strategy: Strategy, b: &Board, c: Color, rng: &mut impl Rng) -> Option<Move> {
    let moves = rules::legal_moves(b, c);
    match strategy {
        Strategy::Random => moves.choose(rng).copied(),
        Strategy::Greedy => greedy(b, c, &moves, rng),
    }
}

fn greedy(b: &Board, c: Color, moves: &[Move], rng: &mut impl Rng) -> Option<Move> {
    let mut b = b.clone();
    let mut best = Vec::new();
    let mut best_score = i32::MIN;
    for &m in moves {
        b.apply(m);
        let won = !rules::has_moves(&b, c.other());
        let score = mobility(&b, c) - mobility(&b, c.other());
        b.undo(m);
        if won {
            return Some(m);
        }
        if score > best_score {
            best_score = score;
            best.clear();
        }
        if score == best_score {
            best.push(m);
        }
    }
    best.choose(rng).copied()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::tests::diagram;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn both_strategies_play_legal_moves() {
        let b = Board::initial();
        let mut rng = StdRng::seed_from_u64(1);
        for s in [Strategy::Random, Strategy::Greedy] {
            let m = choose(s, &b, Color::Red, &mut rng).unwrap();
            assert!(rules::is_legal(&b, Color::Red, m), "{s:?}");
        }
    }

    #[test]
    fn greedy_takes_immediate_win() {
        // Noir en A12 n'a qu'une sortie (B11), rouge gagne en la detruisant.
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
        let m = choose(Strategy::Greedy, &b, Color::Red, &mut rng).unwrap();
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
                let m = choose(s, &b, turn, &mut rng).expect("le joueur au trait a toujours un coup");
                b.apply(m);
                if let Some(w) = rules::winner_after(&b, turn) {
                    break w;
                }
                turn = turn.other();
            };
            greedy_wins += (winner == greedy_color) as u32;
        }
        assert!(greedy_wins >= 16, "glouton : {greedy_wins}/20");
    }

    #[test]
    fn parses_names() {
        assert_eq!("greedy".parse(), Ok(Strategy::Greedy));
        assert!("2".parse::<Strategy>().is_err());
    }
}
