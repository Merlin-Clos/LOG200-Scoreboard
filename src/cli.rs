//! Lecture minimale des arguments `--nom valeur` et `--drapeau` des binaires.

use std::str::FromStr;

pub struct Args {
    args: Vec<String>,
}

impl Args {
    pub fn from_env() -> Args {
        Args {
            args: std::env::args().skip(1).collect(),
        }
    }

    pub fn flag(&self, name: &str) -> bool {
        self.args.iter().any(|a| a == name)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        let i = self.args.iter().position(|a| a == name)?;
        self.args.get(i + 1).map(String::as_str)
    }

    /// Valeur convertie, ou `default` si l'option est absente. Quitte le programme si elle est invalide.
    pub fn parse<T: FromStr>(&self, name: &str, default: T) -> T {
        match self.get(name) {
            None => default,
            Some(v) => v
                .parse()
                .unwrap_or_else(|_| die(&format!("valeur invalide pour {name} : {v:?}"))),
        }
    }

    pub fn require(&self, name: &str) -> &str {
        self.get(name)
            .unwrap_or_else(|| die(&format!("option manquante : {name}")))
    }
}

pub fn die(msg: &str) -> ! {
    eprintln!("erreur : {msg}");
    std::process::exit(2)
}
