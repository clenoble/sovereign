pub mod error;
pub mod schema;
pub mod surreal;
pub mod traits;

pub use error::{DbError, DbResult};
pub use traits::GraphDB;
