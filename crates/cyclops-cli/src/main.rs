//! The `cyclops` binary.

use cyclops_cli::Environment;
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    cyclops_cli::run(env::args(), &Environment::from_process())
}
