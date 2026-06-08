#![forbid(unsafe_code)]

/*!
Rust-native supramolecular topology and construction kernel.

Phase 0 status:

- establishes the crate boundary for `patina-stk`
- reserves module landing zones for topology, construction, placement, bonding, and optimizer work
- keeps chemistry-toolkit integration behind explicit ports

Non-goals in this scaffold:

- no Python or RDKit types in the domain crate
- no direct port of `stk` reaction factories or EA layers
- no workflow orchestration logic
*/

pub mod analysis;
pub mod bonding;
pub mod building_block;
pub mod chemistry;
pub mod construction;
pub mod domain;
pub mod functional_group;
pub mod optimizers;
pub mod placement;
pub mod ports;
pub mod topology;
