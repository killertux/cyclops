//! The `cyclops` binary.

use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    cyclops_cli::run(env::args())
}
