//! Une partie complète entre les binaires compilés : arbitre et deux bots.

#![allow(clippy::unwrap_used)]

use std::net::TcpListener;
use std::process::{Command, Stdio};

use isolation::board::{Board, Color};
use isolation::record::GameResult;
use isolation::rules::legal_moves;

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn play(bot_args: &[&str], opening: &str) -> GameResult {
    let port = free_port().to_string();
    let referee = Command::new(env!("CARGO_BIN_EXE_isolation-referee"))
        .args(["--port", &port, "--time-ms", "2000", "--opening", opening])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let bot = |seed: &str| {
        Command::new(env!("CARGO_BIN_EXE_isolation-bot"))
            .args(["--port", &port, "--seed", seed])
            .args(bot_args)
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let mut red = bot("1");
    // L'arbitre attribue les couleurs dans l'ordre de connexion.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut black = bot("2");

    let output = referee.wait_with_output().unwrap();
    assert!(output.status.success());
    assert!(red.wait().unwrap().success(), "divergence vue par le rouge");
    assert!(black.wait().unwrap().success(), "divergence vue par le noir");
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn bots_play_a_full_valid_game() {
    let result = play(&["--strategy", "greedy"], "");
    assert!(result.validate().is_ok());
    assert!(result.opening.is_empty());
    assert!(!result.moves.is_empty());
}

#[test]
fn opening_is_canonicalised_and_probes_find_no_divergence() {
    let mut board = Board::initial();
    let first = legal_moves(&board, Color::Red)[0];
    board.apply(first);
    let second = legal_moves(&board, Color::Black)[0];
    let loose = format!("{} , {second}", first.to_string().replace('-', " ").to_lowercase());

    let result = play(&["--strategy", "random", "--probe-illegal"], &loose);
    assert!(result.validate().is_ok());
    assert_eq!(result.opening, [first.to_string(), second.to_string()]);
    assert!(
        result.invalid.red > 0 && result.invalid.black > 0,
        "les sondes doivent être refusées"
    );
}
