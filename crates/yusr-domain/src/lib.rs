//! Pure domain layer of Yusr (يُسر), an open-source accounting system for charities.
//!
//! Nothing in this crate performs IO: every rule here is deterministic and unit-tested,
//! and the infrastructure and interface layers depend on it, never the reverse.

pub mod audit;
pub mod budget;
pub mod charity;
pub mod error;
pub mod hijri;
mod hijri_table;
pub mod ledger;
pub mod money;
pub mod receipt;
pub mod scoring;
pub mod voucher;
pub mod zakat;
pub mod zatca;

pub use error::DomainError;
