//! Shared test helpers for skill unit tests.
//!
//! Available only under `#[cfg(test)]` — these helpers ship with the
//! crate's tests, not its release binary.

#![cfg(test)]

use std::collections::HashSet;
use std::sync::Arc;

use sovereign_core::content::ContentFields;

use crate::manifest::Capability;
use crate::traits::{SkillContext, SkillDbAccess, SkillDocument, SkillLlmAccess};

/// `SkillContext` with no capabilities, no DB, no LLM. Use for skills
/// that don't touch the workspace.
pub fn dummy_ctx() -> SkillContext {
    SkillContext {
        granted: HashSet::new(),
        db: None,
        llm: None,
    }
}

/// `SkillContext` granting the given capabilities, no DB, no LLM.
pub fn ctx_with_caps(caps: impl IntoIterator<Item = Capability>) -> SkillContext {
    SkillContext {
        granted: caps.into_iter().collect(),
        db: None,
        llm: None,
    }
}

/// `SkillContext` carrying a DB stub and the capabilities it requires.
pub fn ctx_with_db(
    caps: impl IntoIterator<Item = Capability>,
    db: Arc<dyn SkillDbAccess>,
) -> SkillContext {
    SkillContext {
        granted: caps.into_iter().collect(),
        db: Some(db),
        llm: None,
    }
}

/// `SkillContext` carrying both DB and LLM stubs.
pub fn ctx_with_db_and_llm(
    caps: impl IntoIterator<Item = Capability>,
    db: Arc<dyn SkillDbAccess>,
    llm: Arc<dyn SkillLlmAccess>,
) -> SkillContext {
    SkillContext {
        granted: caps.into_iter().collect(),
        db: Some(db),
        llm: Some(llm),
    }
}

/// `SkillDocument` with id `document:test` and title `Test`, given body.
pub fn make_doc(body: &str) -> SkillDocument {
    make_doc_with_title("Test", body)
}

/// `SkillDocument` with id `document:test`, given title and body.
pub fn make_doc_with_title(title: &str, body: &str) -> SkillDocument {
    SkillDocument {
        id: "document:test".into(),
        title: title.into(),
        content: ContentFields {
            body: body.into(),
            ..Default::default()
        },
    }
}
