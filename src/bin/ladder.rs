//! isolation-ladder : enchaine les parties entre equipes et ajoute une ligne par partie au JSONL.
//! Une seule partie a la fois, sur un seul port, parce que les jars peuvent coder le port en dur.
//!
//! Usage : isolation-ladder ladder.toml [--rounds N]   (N paires, sans fin par defaut)

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use rand::Rng;
use rand::seq::{IndexedRandom, SliceRandom};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use isolation::board::{Board, Color};
use isolation::cli::{Args, die};
use isolation::record::{GameRecord, GameResult, Player, now_rfc3339, parse_jsonl};
use isolation::rules;

#[derive(Deserialize)]
struct Config {
    /// `{port}`, `{time_ms}` et `{opening}` sont remplaces.
    referee: Vec<String>,
    #[serde(default = "default_port")]
    port: u16,
    time_ms: u64,
    session: String,
    #[serde(default = "default_out")]
    out: String,
    /// Demi-coups aleatoires imposes en debut de partie (pair). 0 si un client ignore le plateau initial.
    #[serde(default = "default_opening")]
    opening_plies: usize,
    /// Debuts de lignes attendus sur le stderr de l'arbitre. Vides : attente fixe de `startup_ms`.
    #[serde(default = "default_ready")]
    ready_marker: String,
    #[serde(default = "default_connected")]
    red_connected_marker: String,
    #[serde(default = "default_startup")]
    startup_ms: u64,
    /// Lancee apres chaque partie, reussie ou non (ex. `docker rm -f` des conteneurs). `{game}` est remplace.
    /// Aussi lancee au demarrage avec `{game}` vide, pour ramasser les restes d'un arret brutal.
    #[serde(default)]
    cleanup: Vec<String>,
    /// Duree max d'une partie. Par defaut : 144 coups au temps max, plus 2 minutes.
    game_timeout_s: Option<u64>,
    /// Exporte en variables d'environnement `SANDBOX_<CLE>` pour l'arbitre, les joueurs et `cleanup`.
    #[serde(default)]
    sandbox: BTreeMap<String, toml::Value>,
    #[serde(rename = "team")]
    teams: Vec<Team>,
}

#[derive(Deserialize, Clone)]
struct Team {
    id: String,
    /// `{port}`, `{game}` et `{side}` (red ou black) sont remplaces.
    cmd: Vec<String>,
    /// Fichier dont le sha256 identifie la soumission. Par defaut, le premier `.jar` de `cmd`.
    build: Option<String>,
    #[serde(default)]
    anchor: bool,
}

fn default_port() -> u16 {
    8888
}
fn default_out() -> String {
    "data/games.jsonl".into()
}
fn default_opening() -> usize {
    2
}
fn default_ready() -> String {
    "LISTENING".into()
}
fn default_connected() -> String {
    "CONNECTED red".into()
}
fn default_startup() -> u64 {
    1000
}

/// Au-dela, le journal d'un joueur est tronque : un jar ne doit pas pouvoir remplir le disque.
const LOG_CAP: usize = 1 << 20;
/// Pause apres une partie annulee : une config cassee ne doit pas faire tourner le ladder a vide.
const RETRY_DELAY: Duration = Duration::from_secs(5);

fn main() {
    let args = Args::from_env();
    let path = std::env::args()
        .nth(1)
        .filter(|a| !a.starts_with("--"))
        .unwrap_or_else(|| die("usage : isolation-ladder ladder.toml [--rounds N]"));
    let rounds: u64 = args.parse("--rounds", 0);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| die(&format!("{path} : {e}")));
    let cfg: Config = toml::from_str(&text).unwrap_or_else(|e| die(&format!("{path} : {e}")));
    if cfg.teams.len() < 2 {
        die("il faut au moins deux equipes");
    }
    if !cfg.opening_plies.is_multiple_of(2) {
        die("opening_plies doit etre pair");
    }
    if let Some(dir) = std::path::Path::new(&cfg.out).parent() {
        fs::create_dir_all(dir.join("logs")).unwrap_or_else(|e| die(&format!("{}: {e}", dir.display())));
    }

    if !cfg.cleanup.is_empty() {
        let vars = Vars {
            cfg: &cfg,
            opening: String::new(),
            game: "",
        };
        drop(Cleanup(vars.command(&cfg.cleanup, "").ok()));
    }

    let mut played = pair_counts(&cfg);
    let mut rng = rand::rng();
    let mut round = 0;
    while rounds == 0 || round < rounds {
        round += 1;
        let (a, b) = next_pair(&cfg.teams, &played, &mut rng);
        let opening = random_opening(cfg.opening_plies, &mut rng);
        eprintln!(
            "== paire {round} : {} contre {} (ouverture {})",
            a.id,
            b.id,
            opening.join(",")
        );
        for (red, black) in [(a, b), (b, a)] {
            match play(&cfg, red, black, &opening) {
                Ok(record) => {
                    append(&cfg.out, &record);
                    eprintln!(
                        "   {} (rouge) contre {} (noir) : {} gagne ({:?}, {} coups)",
                        red.id,
                        black.id,
                        record.result.winner.name(),
                        record.result.reason,
                        record.result.moves.len()
                    );
                }
                Err(e) => {
                    eprintln!("   partie {} contre {} annulee : {e}", red.id, black.id);
                    std::thread::sleep(RETRY_DELAY);
                }
            }
        }
        *played.entry(key(&a.id, &b.id)).or_default() += 2;
    }
}

fn key(a: &str, b: &str) -> (String, String) {
    if a < b {
        (a.into(), b.into())
    } else {
        (b.into(), a.into())
    }
}

/// Parties deja jouees par paire dans la session, pour reprendre la ou le ladder s'est arrete.
fn pair_counts(cfg: &Config) -> HashMap<(String, String), u32> {
    let mut counts = HashMap::new();
    if let Ok(text) = fs::read_to_string(&cfg.out) {
        for g in parse_jsonl(&text).0.iter().filter(|g| g.session == cfg.session) {
            *counts.entry(key(&g.red.team, &g.black.team)).or_default() += 1;
        }
    }
    counts
}

fn next_pair<'a>(
    teams: &'a [Team],
    played: &HashMap<(String, String), u32>,
    rng: &mut impl Rng,
) -> (&'a Team, &'a Team) {
    let mut pairs: Vec<(&Team, &Team, u32)> = Vec::new();
    for (i, a) in teams.iter().enumerate() {
        for b in &teams[i + 1..] {
            pairs.push((a, b, played.get(&key(&a.id, &b.id)).copied().unwrap_or(0)));
        }
    }
    let min = pairs.iter().map(|p| p.2).min().unwrap_or(0);
    pairs.retain(|p| p.2 == min);
    let &(a, b, _) = pairs.choose(rng).expect("au moins une paire");
    if rng.random() { (a, b) } else { (b, a) }
}

fn random_opening(plies: usize, rng: &mut impl Rng) -> Vec<String> {
    let mut b = Board::initial();
    let mut turn = Color::Red;
    let mut out = Vec::new();
    for _ in 0..plies {
        let mut moves = rules::legal_moves(&b, turn);
        moves.shuffle(rng);
        let m = moves[0];
        b.apply(m);
        out.push(m.to_string());
        turn = turn.other();
    }
    out
}

/// Valeurs remplacees dans les gabarits de commande pour une partie.
struct Vars<'a> {
    cfg: &'a Config,
    opening: String,
    game: &'a str,
}

impl Vars<'_> {
    fn apply(&self, template: &[String], side: &str) -> Vec<String> {
        template
            .iter()
            .map(|a| {
                a.replace("{port}", &self.cfg.port.to_string())
                    .replace("{time_ms}", &self.cfg.time_ms.to_string())
                    .replace("{opening}", &self.opening)
                    .replace("{game}", self.game)
                    .replace("{side}", side)
            })
            .collect()
    }

    fn command(&self, template: &[String], side: &str) -> std::io::Result<Command> {
        let cmd = self.apply(template, side);
        let (prog, rest) = cmd
            .split_first()
            .ok_or_else(|| std::io::Error::other("commande vide"))?;
        let mut c = Command::new(prog);
        c.args(rest).stdin(Stdio::null());
        for (k, v) in &self.cfg.sandbox {
            let v = match v {
                toml::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            c.env(format!("SANDBOX_{}", k.to_uppercase()), v);
        }
        Ok(c)
    }
}

/// Tue un processus a la sortie de portee (joueurs restes en vie, arbitre en cas d'erreur), ainsi que
/// ses enfants directs : un script d'enrobage ne doit pas laisser d'orphelins. On reste dans le meme
/// groupe de processus pour que Ctrl+C atteigne tout le monde.
struct Reaper(Child);

impl Drop for Reaper {
    fn drop(&mut self) {
        let _ = Command::new("pkill")
            .args(["-KILL", "-P", &self.0.id().to_string()])
            .status();
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Lance `cleanup` a la sortie de portee, apres les `Reaper` declares plus tard.
struct Cleanup(Option<Command>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        if let Some(c) = &mut self.0 {
            let _ = c.stdout(Stdio::null()).stderr(Stdio::null()).status();
        }
    }
}

/// Joueur dont la sortie d'erreur va dans `log`, tronquee a `LOG_CAP` octets.
fn spawn_player(mut cmd: Command, log: std::path::PathBuf) -> std::io::Result<Reaper> {
    let mut child = cmd.stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
    let mut stderr = child.stderr.take().expect("stderr pipe");
    let mut file = fs::File::create(log)?;
    std::thread::spawn(move || {
        let mut kept = 0;
        let mut buf = [0u8; 8192];
        // On continue de lire apres le plafond pour ne pas bloquer le joueur sur un tube plein.
        while let Ok(n) = stderr.read(&mut buf) {
            if n == 0 {
                break;
            }
            if kept < LOG_CAP {
                let take = n.min(LOG_CAP - kept);
                let _ = file.write_all(&buf[..take]);
                kept += take;
                if kept == LOG_CAP {
                    let _ = file.write_all(b"\n[journal tronque]\n");
                }
            }
        }
    });
    Ok(Reaper(child))
}

fn play(cfg: &Config, red: &Team, black: &Team, opening: &[String]) -> Result<GameRecord, String> {
    let ts = now_rfc3339();
    let id = format!("{}-{}-{}", ts.replace([':', '-'], ""), red.id, black.id);
    let logs = std::path::Path::new(&cfg.out)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("logs");
    let vars = Vars {
        cfg,
        opening: opening.join(","),
        game: &id,
    };
    let _cleanup = Cleanup(if cfg.cleanup.is_empty() {
        None
    } else {
        vars.command(&cfg.cleanup, "").ok()
    });

    let mut referee = vars
        .command(&cfg.referee, "")
        .and_then(|mut c| c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn())
        .map_err(|e| format!("lancement de l'arbitre : {e}"))?;
    let stdout = referee.stdout.take().expect("stdout pipe");
    let stderr = referee.stderr.take().expect("stderr pipe");
    let mut referee = Reaper(referee);

    // Les lignes de l'arbitre sont relayees sur notre stderr et passees au thread principal.
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("   [arbitre] {line}");
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let wait_for = |marker: &str| -> Result<bool, String> {
        if marker.is_empty() {
            std::thread::sleep(Duration::from_millis(cfg.startup_ms));
            return Ok(true);
        }
        loop {
            match rx.recv_timeout(Duration::from_secs(60)) {
                Ok(line) if line.trim().starts_with(marker) => return Ok(true),
                Ok(_) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(false),
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(format!("arbitre : `{marker}` jamais recu")),
            }
        }
    };

    let player = |t: &Team, side: &str| {
        vars.command(&t.cmd, side)
            .and_then(|c| spawn_player(c, logs.join(format!("{id}-{}.log", t.id))))
            .map_err(|e| format!("lancement de {} : {e}", t.id))
    };
    wait_for(&cfg.ready_marker)?;
    let _red = player(red, "red")?;
    let _black = if wait_for(&cfg.red_connected_marker)? {
        Some(player(black, "black")?)
    } else {
        None
    };

    // Lecture dans un thread pour pouvoir abandonner une partie qui ne finit jamais.
    let (json_tx, json_rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let mut json = String::new();
        let _ = BufReader::new(stdout).read_line(&mut json);
        let _ = json_tx.send(json);
    });
    let timeout = Duration::from_secs(cfg.game_timeout_s.unwrap_or(144 * cfg.time_ms / 1000 + 120));
    let json = json_rx
        .recv_timeout(timeout)
        .map_err(|_| format!("partie interrompue apres {timeout:?}"))?;
    let status = referee.0.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("arbitre termine avec {status}"));
    }
    let result: GameResult = serde_json::from_str(&json).map_err(|e| format!("resultat illisible ({e}) : {json:?}"))?;
    result.validate().map_err(|e| format!("resultat incoherent : {e}"))?;

    let entry = |t: &Team| Player {
        team: t.id.clone(),
        build: build_hash(t),
        anchor: t.anchor,
    };
    Ok(GameRecord {
        id: id.clone(),
        session: cfg.session.clone(),
        ts,
        red: entry(red),
        black: entry(black),
        result,
    })
}

fn build_hash(t: &Team) -> Option<String> {
    let file = t
        .build
        .clone()
        .or_else(|| t.cmd.iter().find(|a| a.ends_with(".jar")).cloned())?;
    let bytes = fs::read(&file).ok()?;
    let digest = Sha256::digest(&bytes);
    Some(digest.iter().take(8).map(|b| format!("{b:02x}")).collect())
}

/// Une ligne = une seule ecriture suivie d'un flush : un lecteur concurrent voit au pire une
/// derniere ligne incomplete, qu'il ignore.
fn append(path: &str, record: &GameRecord) {
    let mut line = serde_json::to_string(record).expect("serialisation");
    line.push('\n');
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|e| die(&format!("{path} : {e}")));
    f.write_all(line.as_bytes())
        .and_then(|_| f.sync_data())
        .unwrap_or_else(|e| die(&format!("{path} : {e}")));
}
