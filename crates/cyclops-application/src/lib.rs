//! The Cyclops use cases: the ports the outside world plugs into, the analysis
//! pipeline, and the analyzers that turn facts into metrics.

pub mod analysis;
pub mod analyzers;
pub mod browse;
pub mod diagram;
pub mod duplication;
pub mod graph;
pub mod history;
pub mod pipeline;
pub mod ports;
pub mod precise;
pub mod queries;
pub mod rollups;
