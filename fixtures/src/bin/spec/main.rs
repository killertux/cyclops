//! The Slim server that runs the specification in `specs/`.
//!
//! No fixture exists yet, so every snoozed table reports as snoozed. Register each
//! fixture here as its behavior is built. See `specs/writing-specs.md`.

use rust_slim::PortSlimServer;
use std::env::args;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let server = PortSlimServer::listen_from_args(args().skip(1))?;
    server.run()?;
    Ok(())
}
