pub mod error;
pub mod schema;
pub mod surreal;
pub mod traits;

#[cfg(feature = "encryption")]
pub mod encrypted;

pub use error::{DbError, DbResult};
pub use traits::GraphDB;
