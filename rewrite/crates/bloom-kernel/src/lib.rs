#![deny(clippy::disallowed_methods)]

pub mod geometry;
pub mod invariants;
pub mod ports;

#[cfg(any(test, feature = "simulation"))]
pub mod simulation;
