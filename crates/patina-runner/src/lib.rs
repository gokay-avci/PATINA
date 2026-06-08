#![forbid(unsafe_code)]

/*!
What this crate implements: the control-plane runner that maps independent candidates onto
isolated subprocess sandboxes using Rayon.
Design basis: prompt Sections 2, 5, 6, and 7 require process-level evaluation, configurable
worker counts, cleanup control, and partial-failure isolation.
Assumption: each `evaluate_population` invocation owns a unique run directory under the base
workdir so that two concurrent Python or CLI calls do not race on `worker_0001` paths.
*/

mod config;
mod dispatch;
mod error;
mod pool;
mod runtime;

pub use config::RunConfig;
pub use dispatch::{evaluate_population, evaluate_requests};
pub use error::RunnerError;
pub use pool::PersistentWorkerPool;

#[cfg(test)]
mod tests;
