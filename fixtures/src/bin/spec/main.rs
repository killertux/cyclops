//! The Slim server that runs the specification in `specs/`.
//!
//! See `specs/writing-specs.md` for how the fixtures are used. Fixtures grow as behavior
//! is built, and a snoozed table may use what is not here yet.

mod cli;
mod fixtures;
mod read;
mod scenario;
mod scratch;

use fixtures::{Cli, Core, Snippet};
use rust_slim::PortSlimServer;
use std::env::args;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut server = PortSlimServer::listen_from_args(args().skip(1))?;
    server.add_fixture::<Snippet>();
    server.add_fixture::<Core>();
    server.add_fixture::<Cli>();
    server.run()?;
    Ok(())
}
