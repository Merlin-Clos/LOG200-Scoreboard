//! Formats decrits dans docs/formats.md. `GameResult` est ce que l'arbitre ecrit sur stdout,
//! `GameRecord` la ligne du JSONL, completee par le ladder. Les versions inconnues sont ignorees.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::board::Color;
use crate::rules;

pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    NoMoves,
    Timeout,
    /// Connexion fermee ou processus termine en cours de partie.
    Disconnect,
    /// Le joueur n'a jamais envoye de coup valide malgre les `4` (voir `--max-invalid`).
    Protocol,
    /// Le joueur ne s'est jamais connecte (jar qui plante au demarrage, conteneur tue...).
    NoShow,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct InvalidCounts {
    pub red: u32,
    pub black: u32,
}

impl InvalidCounts {
    pub fn add(&mut self, c: Color) {
        match c {
            Color::Red => self.red += 1,
            Color::Black => self.black += 1,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct GameResult {
    pub v: u32,
    pub time_ms: u64,
    /// Demi-coups imposes par l'arbitre avant de donner la main aux joueurs.
    #[serde(default)]
    pub opening: Vec<String>,
    /// Coups joues par les clients, format canonique `G1-E3-C12`.
    pub moves: Vec<String>,
    pub winner: Color,
    pub reason: Reason,
    #[serde(default)]
    pub invalid: InvalidCounts,
    pub duration_ms: u64,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Player {
    pub team: String,
    /// 16 premiers caracteres hex du sha256 du jar. Un nouveau hash marque une nouvelle soumission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    /// Joueur de reference optionnel : rating fixe dans le classement.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub anchor: bool,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct GameRecord {
    pub id: String,
    pub session: String,
    /// RFC 3339, UTC.
    pub ts: String,
    pub red: Player,
    pub black: Player,
    #[serde(flatten)]
    pub result: GameResult,
}

impl GameResult {
    /// Verifie que les coups sont legaux et coherents avec le resultat annonce.
    pub fn validate(&self) -> Result<(), String> {
        let all = self.opening.iter().chain(&self.moves).map(String::as_str);
        let (_, turn, winner) = rules::replay(all).map_err(|(i, m)| format!("coup {i} illegal : {m}"))?;
        match self.reason {
            Reason::NoShow if !self.moves.is_empty() => Err("no_show annonce apres des coups joues".into()),
            Reason::NoShow => Ok(()),
            Reason::NoMoves if winner != Some(self.winner) => {
                Err(format!("no_moves annonce, gagnant rejoue : {winner:?}"))
            }
            Reason::NoMoves => Ok(()),
            // Le joueur au trait a perdu (temps, deconnexion ou coups invalides).
            _ if winner.is_some() => Err("partie deja terminee avant l'abandon annonce".into()),
            _ if self.winner == turn => Err(format!("le gagnant {} etait au trait", turn.name())),
            _ => Ok(()),
        }
    }
}

/// Lit un JSONL en sautant les lignes illisibles (par exemple une derniere ligne en cours
/// d'ecriture) et les versions inconnues. Renvoie aussi leurs numeros (a partir de 1) et la raison.
pub fn parse_jsonl(text: &str) -> (Vec<GameRecord>, Vec<(usize, String)>) {
    let mut games = Vec::new();
    let mut skipped = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<serde_json::Value>(line)
            .map_err(|e| e.to_string())
            .and_then(|v| match v.get("v").and_then(|v| v.as_u64()) {
                Some(1) => serde_json::from_value::<GameRecord>(v).map_err(|e| e.to_string()),
                other => Err(format!("version non geree : {other:?}")),
            });
        match parsed {
            Ok(g) => games.push(g),
            Err(e) => skipped.push((i + 1, e)),
        }
    }
    (games, skipped)
}

pub fn now_rfc3339() -> String {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    rfc3339(d.as_secs() as i64, d.subsec_millis())
}

fn rfc3339(secs: i64, millis: u32) -> String {
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    // Conversion jours -> date civile (algorithme de H. Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::Board;
    use crate::rules::legal_moves;

    fn sample() -> GameRecord {
        let m = legal_moves(&Board::initial(), Color::Red)[0].to_string();
        GameRecord {
            id: "g1".into(),
            session: "test".into(),
            ts: "2026-10-24T12:00:00.000Z".into(),
            red: Player {
                team: "eq01".into(),
                build: Some("abc".into()),
                anchor: false,
            },
            black: Player {
                team: "eq02".into(),
                build: None,
                anchor: true,
            },
            result: GameResult {
                v: VERSION,
                time_ms: 500,
                opening: vec![],
                moves: vec![m],
                winner: Color::Red,
                reason: Reason::Timeout,
                invalid: InvalidCounts { red: 0, black: 1 },
                duration_ms: 1234,
            },
        }
    }

    #[test]
    fn jsonl_roundtrip_skips_truncated_and_unknown_versions() {
        let line = serde_json::to_string(&sample()).unwrap();
        let text = format!("{line}\n{{\"v\":2}}\n{}", &line[..line.len() / 2]);
        let (games, skipped) = parse_jsonl(&text);
        assert_eq!(games, vec![sample()]);
        assert_eq!(skipped.iter().map(|s| s.0).collect::<Vec<_>>(), [2, 3]);
    }

    #[test]
    fn flattened_fields_are_top_level() {
        let v: serde_json::Value = serde_json::to_value(sample()).unwrap();
        assert_eq!(v["winner"], "red");
        assert_eq!(v["reason"], "timeout");
        assert_eq!(v["red"]["team"], "eq01");
        assert!(v["black"].get("build").is_none());
        assert_eq!(v["black"]["anchor"], true);
        assert!(v["red"].get("anchor").is_none());
    }

    #[test]
    fn validate_checks_winner_consistency() {
        // Apres un coup rouge, c'est au noir : un timeout le fait perdre.
        assert!(sample().result.validate().is_ok());
        let mut wrong = sample().result;
        wrong.winner = Color::Black;
        assert!(wrong.validate().is_err());
        let mut no_moves = sample().result;
        no_moves.reason = Reason::NoMoves;
        assert!(no_moves.validate().is_err());
    }

    #[test]
    fn no_show_requires_no_moves_and_accepts_either_winner() {
        let mut absent = sample().result;
        absent.reason = Reason::NoShow;
        assert!(absent.validate().is_err());
        absent.moves.clear();
        for winner in [Color::Red, Color::Black] {
            absent.winner = winner;
            assert!(absent.validate().is_ok(), "{winner:?}");
        }
        let v: serde_json::Value = serde_json::to_value(Reason::NoShow).unwrap();
        assert_eq!(v, "no_show");
    }

    #[test]
    fn rfc3339_known_dates() {
        assert_eq!(rfc3339(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339(1_793_017_330, 42), "2026-10-26T12:22:10.042Z");
        assert_eq!(rfc3339(951_782_400, 0), "2000-02-29T00:00:00.000Z");
    }
}
