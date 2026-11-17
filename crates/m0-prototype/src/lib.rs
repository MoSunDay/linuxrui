//! Pure, UI-free pieces of the M0 prototype: summary text and the CLI
//! contract. They live in a lib target so both the binary and the test
//! suites share one implementation (no duplicated bin-side modules).
pub mod app_index;
pub mod cli;
pub mod summary;
