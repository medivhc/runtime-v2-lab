//! Domain layer: IDs, roles, plan validation, state machines, scheduling
//! and authorization rules. Pure logic, no I/O.

pub mod error;
pub mod ids;
pub mod plan;
pub mod roles;
pub mod state;
pub mod sched;

pub use error::DomainError;
pub use roles::Role;
pub use state::*;
