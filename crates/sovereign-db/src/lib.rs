pub mod document_titles;
pub mod error;
pub mod layered;
pub mod schema;
pub mod surreal;
pub mod thread_names;
pub mod traits;

#[cfg(feature = "encryption")]
pub mod encrypted;

#[cfg(any(test, feature = "test-utils"))]
pub mod mock;

pub use document_titles::{resolve_document_title, same_document_title, DocumentTitleMatch, MAX_CLOSE_TITLES};
pub use error::{DbError, DbResult};
pub use thread_names::{
    existing_thread_for_find_or_create, resolve_thread_name, same_thread_name, ThreadNameMatch, MAX_CLOSE_NAMES,
};
pub use traits::GraphDB;
