//! `peabiru`: the routing engine of peabiru (`peabiru-core`) from the
//! shell. See `peabiru --help`, and the command line chapter of the Rust
//! guide.

mod access;
mod app;
mod common;
mod inspect;
mod legacy;
mod matrix;
mod trips;

use app::{Cli, Command};
use clap::{CommandFactory, Parser};
use std::sync::atomic::{AtomicBool, Ordering};

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    if legacy::is_legacy(&args) {
        legacy::run(&args);
        return;
    }
    let cli = Cli::parse();
    // Ctrl-C cancels the computation, which ends with status 130; a second
    // one ends the process at once
    static STOPPING: AtomicBool = AtomicBool::new(false);
    let token = common::cancel_token().clone();
    let _ = ctrlc::set_handler(move || {
        if STOPPING.swap(true, Ordering::SeqCst) {
            std::process::exit(130);
        }
        token.cancel();
    });
    let (name, r) = match cli.command {
        Command::Info(a) => ("info", inspect::info(a)),
        Command::Matrix(a) => ("matrix", matrix::matrix(a)),
        Command::Expanded(a) => ("expanded", matrix::expanded(a)),
        Command::Arrival(a) => ("arrival", matrix::arrival(a)),
        Command::Accessibility(a) => ("accessibility", access::accessibility(a)),
        Command::Isochrone(a) => ("isochrone", access::isochrone(a)),
        Command::Itineraries(a) => ("itineraries", trips::itineraries(a)),
        Command::Pareto(a) => ("pareto", trips::pareto(a)),
        Command::Options(a) => ("options", trips::options(a)),
        Command::Snap(a) => ("snap", inspect::snap(a)),
        Command::Network(a) => ("network", inspect::network(a)),
        Command::Fares(c) => ("fares", inspect::fares(c)),
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "peabiru",
                &mut std::io::stdout(),
            );
            ("completions", Ok(()))
        }
    };
    if let Err(e) = r {
        let msg = if STOPPING.load(Ordering::SeqCst) && e.code == 130 {
            "interrupted".to_string()
        } else {
            e.msg
        };
        eprintln!("peabiru {name}: {msg}");
        std::process::exit(e.code);
    }
}
