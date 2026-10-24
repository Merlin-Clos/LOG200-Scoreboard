//! isolation-check : rejoue avec notre moteur les parties d'un JSONL du ladder ou d'une sauvegarde du
//! serveur officiel (`BOARDGAME 1`).
//!
//! Usage : isolation-check FICHIER...   (code de sortie 1 si une partie est invalide)

use isolation::record::parse_jsonl;
use isolation::rules;

fn main() {
    let files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        eprintln!("usage : isolation-check FICHIER...");
        std::process::exit(2);
    }
    let mut failures = 0;
    for path in &files {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path} : {e}");
                failures += 1;
                continue;
            }
        };
        let errors = if text.starts_with("BOARDGAME") {
            check_save(&text)
        } else {
            check_jsonl(&text)
        };
        if errors.is_empty() {
            println!("{path} : OK");
        }
        for e in &errors {
            println!("{path} : {e}");
        }
        failures += errors.len();
    }
    if failures > 0 {
        std::process::exit(1);
    }
}

fn check_jsonl(text: &str) -> Vec<String> {
    let (games, skipped) = parse_jsonl(text);
    let mut errors: Vec<String> = skipped
        .into_iter()
        .map(|(line, e)| format!("ligne {line} ignoree : {e}"))
        .collect();
    for g in &games {
        if let Err(e) = g.result.validate() {
            errors.push(format!("partie {} : {e}", g.id));
        }
    }
    eprintln!("{} partie(s) lue(s)", games.len());
    errors
}

/// Format : `BOARDGAME 1`, `rows=`, `cols=`, `start=`, `current=`, `moves=n`, puis n coups.
fn check_save(text: &str) -> Vec<String> {
    let mut lines = text.lines().map(str::trim);
    let mut declared = None;
    for line in lines.by_ref() {
        if let Some(n) = line.strip_prefix("moves=") {
            declared = n.parse::<usize>().ok();
            break;
        }
        if line.starts_with("rows=") && line != "rows=12" || line.starts_with("cols=") && line != "cols=12" {
            return vec![format!("plateau non gere : {line}")];
        }
    }
    let Some(n) = declared else {
        return vec!["entete moves= introuvable".into()];
    };
    let moves: Vec<&str> = lines.filter(|l| !l.is_empty()).collect();
    if moves.len() != n {
        return vec![format!("{n} coups annonces, {} lus", moves.len())];
    }
    match rules::replay(moves.iter().copied()) {
        Ok((_, _, winner)) => {
            let status = winner.map_or("en cours".to_string(), |w| format!("gagnant {}", w.name()));
            eprintln!("{n} coups, {status}");
            vec![]
        }
        Err((i, m)) => vec![format!("coup {} illegal : {m}", i + 1)],
    }
}
