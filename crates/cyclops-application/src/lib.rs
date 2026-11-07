//! The Cyclops use cases: the ports the outside world plugs into, the analysis
//! pipeline, and the analyzers that turn facts into metrics.

pub mod analysis;
pub mod analyzers;
pub mod graph;
pub mod pipeline;
pub mod ports;
pub mod rollups;
