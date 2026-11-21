//! Profils des jars d'équipe : les lignes `prof` du profileur (`sandbox/profiler/`, d'après l'arène
//! isola de Justin Pagès, gitlab.com/justin-pages/isola) et les lignes `ARENA` du jar, lues dans son
//! journal à la fin de la partie. Privés : seule l'équipe les voit, sauf la part du gabarit qu'elle
//! choisit de publier.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use serde::Serialize;

use isolation_core::board::Color;
use isolation_core::record::Measure;

use crate::measures;

/// Les `Classe.méthode` du code de départ du cours.
static TEMPLATE: LazyLock<HashSet<&str>> = LazyLock::new(|| {
    include_str!("template.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
});

/// Le profil d'un côté d'une partie.
#[derive(Debug, PartialEq)]
pub struct Profile {
    pub color: Color,
    pub team: String,
    pub build: String,
    /// Lignes `prof` : une par segment de calcul (deux tours rapides peuvent n'en faire qu'un).
    pub segments: usize,
    pub moves: usize,
    pub overhead_ms: u64,
    pub tree: Tree,
    /// Les lignes `ARENA` du jar, telles quelles.
    pub arena: Vec<Measure>,
    /// La part du gabarit, si l'équipe la publie.
    pub template: Option<TemplateShare>,
}

/// Les échantillons d'une partie, sommés sur ses segments comme dans l'arène d'isola.
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct Tree {
    /// Piles de threads lues.
    pub samples: u64,
    /// Temps passé à calculer selon le profileur.
    pub sampled_ms: u64,
    /// Temps de réflexion du jar, la somme de ses `move_ms`.
    pub think_ms: u64,
    /// Le profileur a vu moins de la moitié de ce temps : thread lecteur dédié, threads virtuels.
    pub doubtful: bool,
    /// Profondeur de récursion la plus grande.
    pub rec: u32,
    /// `Classe.méthode` → `[self, total]`.
    pub methods: BTreeMap<String, [u64; 2]>,
    /// Chemin d'appel `A.f>B.g`, récursion repliée → échantillons dont c'est la pile.
    pub paths: BTreeMap<String, u64>,
}

/// Échantillons en propre des méthodes du gabarit, sur `samples` : le reste va aux « autres ».
#[derive(Debug, PartialEq, Serialize)]
pub struct TemplateShare {
    pub samples: u64,
    pub methods: BTreeMap<String, u64>,
}

/// Ce que la partie dit du côté profilé.
pub struct Side<'a> {
    pub color: Color,
    pub team: &'a str,
    pub build: &'a str,
    /// `move_ms` de la partie, les deux couleurs : le rouge joue le premier coup client.
    pub move_ms: &'a [u64],
    pub public: bool,
}

/// Ajoute une ligne `prof n=… t=… rec=… cost=… end=1 Classe.méthode=self:total … @chemin=n …` ;
/// rend son `cost` (µs). Les jetons illisibles sont ignorés.
fn add(tree: &mut Tree, line: &str) -> u64 {
    let mut cost = 0;
    for (key, value) in line.split(' ').filter_map(|t| t.rsplit_once('=')) {
        let number = || value.parse::<u64>().unwrap_or_default();
        match key {
            "n" => tree.samples += number(),
            "t" => tree.sampled_ms += number(),
            "rec" => tree.rec = tree.rec.max(value.parse().unwrap_or_default()),
            "cost" => cost += number(),
            "end" | "" => {}
            _ => {
                if let Some(path) = key.strip_prefix('@') {
                    *tree.paths.entry(path.to_owned()).or_default() += number();
                } else if let Some((Ok(own), Ok(total))) = value
                    .split_once(':')
                    .map(|(own, total)| (own.parse::<u64>(), total.parse::<u64>()))
                {
                    let counts = tree.methods.entry(key.to_owned()).or_default();
                    counts[0] += own;
                    counts[1] += total;
                }
            }
        }
    }
    cost
}

/// Le profil du journal `log` d'un côté profilé.
fn of_log(log: &str, side: &Side) -> Profile {
    let mut tree = Tree::default();
    let (mut segments, mut cost_us) = (0, 0);
    for line in log.lines().filter_map(|l| l.strip_prefix("prof ")) {
        segments += 1;
        cost_us += add(&mut tree, line);
    }
    let first = usize::from(side.color == Color::Black);
    let own: Vec<u64> = side.move_ms.iter().skip(first).step_by(2).copied().collect();
    tree.think_ms = own.iter().sum();
    tree.doubtful = tree.sampled_ms * 2 < tree.think_ms;
    let template = side.public.then(|| TemplateShare {
        samples: tree.samples,
        methods: tree
            .methods
            .iter()
            .filter(|(method, _)| TEMPLATE.contains(method.as_str()))
            .map(|(method, counts)| (method.clone(), counts[0]))
            .collect(),
    });
    Profile {
        color: side.color,
        team: side.team.to_owned(),
        build: side.build.to_owned(),
        segments,
        moves: own.len(),
        overhead_ms: cost_us / 1000,
        arena: log.lines().filter_map(measures::parse).collect(),
        tree,
        template,
    }
}

/// Le profil du journal `path` ; `None` s'il est illisible.
pub fn read(path: &Path, side: &Side) -> Option<Profile> {
    let bytes = fs::read(path).ok()?;
    Some(of_log(&String::from_utf8_lossy(&bytes), side))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "nouvelle partie : noir, 1000 ms\n\
        partiel sans fin de ligne\n\
        prof n=30 t=300 rec=4 cost=1500 end=1 Client.main=0:30 Ia.search=20:28 GameConnection.readMessage=2:2 @Client.main>Ia.search=20 @Client.main=10\n\
        ARENA depth=6 nodes=1200\n\
        prof n=10 t=100 rec=7 cost=700 end=1 Ia.search=8:10 Client.main=1:10 dégât @Client.main>Ia.search=8 x=y:z\n\
        ARENA nodes=5\n";

    fn side(public: bool) -> Side<'static> {
        Side {
            color: Color::Black,
            team: "eq07",
            build: "h1",
            move_ms: &[999, 300, 999, 450],
            public,
        }
    }

    #[test]
    fn the_segments_of_a_game_are_summed_like_in_isola() {
        let profile = of_log(LOG, &side(false));
        let methods = BTreeMap::from([
            ("Client.main".to_owned(), [1, 40]),
            ("GameConnection.readMessage".to_owned(), [2, 2]),
            ("Ia.search".to_owned(), [28, 38]),
        ]);
        let paths = BTreeMap::from([("Client.main".to_owned(), 10), ("Client.main>Ia.search".to_owned(), 28)]);
        assert_eq!(
            profile,
            Profile {
                color: Color::Black,
                team: "eq07".into(),
                build: "h1".into(),
                segments: 2,
                moves: 2,
                overhead_ms: 2,
                tree: Tree {
                    samples: 40,
                    sampled_ms: 400,
                    think_ms: 750,
                    doubtful: false,
                    rec: 7,
                    methods,
                    paths,
                },
                arena: vec![
                    Measure {
                        depth: Some(6),
                        nodes: 1200
                    },
                    Measure { depth: None, nodes: 5 }
                ],
                template: None,
            }
        );
    }

    #[test]
    fn under_half_of_the_thinking_time_seen_the_profile_is_doubtful() {
        let slow = Side {
            move_ms: &[0, 500, 0, 400],
            ..side(false)
        };
        assert!(of_log(LOG, &slow).tree.doubtful, "400 ms vus sur 900");
        let empty = of_log("fin\n", &side(false));
        assert_eq!(
            (empty.segments, empty.tree.doubtful),
            (0, true),
            "un thread lecteur dédié"
        );
        assert!(
            !of_log(
                "",
                &Side {
                    move_ms: &[],
                    ..side(false)
                }
            )
            .tree
            .doubtful,
            "aucun coup"
        );
    }

    #[test]
    fn only_a_published_profile_has_a_template_share_of_its_template_methods() {
        let share = of_log(LOG, &side(true)).template.unwrap();
        let methods = BTreeMap::from([
            ("Client.main".to_owned(), 1),
            ("GameConnection.readMessage".to_owned(), 2),
        ]);
        assert_eq!(share, TemplateShare { samples: 40, methods });
        assert!(TEMPLATE.contains("GameConnection$GameInfo.getSide") && !TEMPLATE.iter().any(|m| m.starts_with('#')));
        assert!(read(Path::new("/absent/journal.log"), &side(true)).is_none());
    }
}
