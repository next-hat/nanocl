pub mod cargo;
pub(crate) mod cargo_replica;
// Staged ahead of the Cargo runtime cutover; production does not consume it
// yet, but its pure unit tests lock the declared-to-effective contract.
#[cfg_attr(not(test), allow(dead_code))]
mod cargo_compiler;
pub mod generic;
pub mod image;
pub mod job;
pub mod network;
pub mod process;
pub mod vm;
