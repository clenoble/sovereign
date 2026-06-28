//! Sample-data seeding.
//!
//! The implementation moved to `sovereign-ai` (`sovereign_ai::seed`) so the
//! native shell can seed the same sample workspace its onboarding offers —
//! previously this lived here and was unreachable from `sovereign-shell`.
//! Re-exported so existing call sites (`seed::seed_if_empty`,
//! `seed::seed_profile_and_history`, `seed::seed_pii_if_empty`) keep working.
pub use sovereign_ai::seed::*;
