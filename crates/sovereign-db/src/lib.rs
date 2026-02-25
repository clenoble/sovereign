pub mod error;
pub mod schema;
pub mod surreal;
pub mod traits;

#[cfg(feature = "encryption")]
pub mod encrypted;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub use error::{DbError, DbResult};
pub use traits::GraphDB;
