//! Internal database layers for optional PHP database extensions.

#[cfg(not(target_family = "wasm"))]
pub mod mysql;
#[cfg(target_family = "wasm")]
pub mod mysql_wasm;
#[cfg(target_family = "wasm")]
pub use mysql_wasm as mysql;

#[cfg(not(target_family = "wasm"))]
pub mod postgres;
#[cfg(target_family = "wasm")]
pub mod postgres_wasm;
#[cfg(target_family = "wasm")]
pub use postgres_wasm as postgres;
