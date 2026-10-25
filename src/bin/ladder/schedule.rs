//! Choix des paires : la moins jouée de la session, pour que le ladder reprenne là où il s'est
//! arrêté.

use std::collections::HashMap;

use rand::seq::IndexedRandom;
use rand::{Rng, RngExt};

use isolation::record::GameRecord;

use crate::config::Team;

/// Paire d'équipes, sans ordre.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct PairKey(String, String);

impl PairKey {
    fn new(a: &str, b: &str) -> Self {
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        Self(first.to_string(), second.to_string())
    }
}

#[derive(Default)]
pub struct Scheduler {
    played: HashMap<PairKey, u32>,
}

impl Scheduler {
    pub fn from_history(games: &[GameRecord], session: &str) -> Self {
        let mut scheduler = Self::default();
        for game in games.iter().filter(|g| g.session == session) {
            scheduler.record(&game.red.team, &game.black.team, 1);
        }
        scheduler
    }

    pub fn record(&mut self, a: &str, b: &str, games: u32) {
        *self.played.entry(PairKey::new(a, b)).or_default() += games;
    }

    fn played(&self, a: &str, b: &str) -> u32 {
        self.played.get(&PairKey::new(a, b)).copied().unwrap_or(0)
    }

    /// Une des paires les moins jouées, dans un ordre au hasard. `None` s'il y a moins de deux équipes.
    pub fn next_pair<'t>(&self, teams: &'t [Team], rng: &mut impl Rng) -> Option<(&'t Team, &'t Team)> {
        let pairs: Vec<(&Team, &Team, u32)> = teams
            .iter()
            .enumerate()
            .flat_map(|(i, a)| teams[i + 1..].iter().map(move |b| (a, b, self.played(&a.id, &b.id))))
            .collect();
        let fewest = pairs.iter().map(|&(.., n)| n).min()?;
        let least_played: Vec<_> = pairs.into_iter().filter(|&(.., n)| n == fewest).collect();
        let &(a, b, _) = least_played.choose(rng)?;
        Some(if rng.random() { (a, b) } else { (b, a) })
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::config::CommandTemplate;

    fn team(id: &str) -> Team {
        Team {
            id: id.into(),
            cmd: CommandTemplate::default(),
            build: None,
            anchor: false,
        }
    }

    #[test]
    fn picks_the_least_played_pair() {
        let teams = [team("a"), team("b"), team("c")];
        let mut scheduler = Scheduler::default();
        scheduler.record("a", "b", 4);
        scheduler.record("c", "a", 2);
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..20 {
            let (x, y) = scheduler.next_pair(&teams, &mut rng).unwrap();
            let mut ids = [x.id.as_str(), y.id.as_str()];
            ids.sort_unstable();
            assert_eq!(ids, ["b", "c"]);
        }
    }

    #[test]
    fn needs_two_teams() {
        let mut rng = StdRng::seed_from_u64(3);
        assert!(Scheduler::default().next_pair(&[team("a")], &mut rng).is_none());
    }
}
