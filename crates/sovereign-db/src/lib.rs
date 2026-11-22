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

// v0.0.13 lot L5, B1 (C-MERGE, spec ch. 03, MG3): an ended thread in the
// name resolution and find-or-create, written before the code by the test
// author.
#[cfg(test)]
#[path = "b1_merge_tests.rs"]
mod b1_merge_tests;

pub use document_titles::{resolve_document_title, same_document_title, DocumentTitleMatch, MAX_CLOSE_TITLES};
pub use error::{DbError, DbResult};
pub use thread_names::{
    ended_day, ended_thread_answer, existing_thread_for_find_or_create, find_or_create_choice, live_end,
    merged_into_name, no_live_end_words, remember_thread_choice, resolve_thread_name, FindOrCreate,
    same_thread_name, ThreadNameMatch, MAX_CLOSE_NAMES,
};
pub use traits::GraphDB;
