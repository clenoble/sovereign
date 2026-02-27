pub mod db_bridge;
pub mod manifest;
pub mod registry;
pub mod skills;
pub mod traits;

pub use db_bridge::wrap_db;
pub use manifest::{Capability, SkillManifest};
pub use registry::SkillRegistry;
pub use traits::{CoreSkill, SkillContext, SkillDbAccess, SkillDocument, SkillOutput};
