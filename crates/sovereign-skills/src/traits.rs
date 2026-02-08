/// Trait for core skills that are compiled into the Sovereign OS binary.
///
/// Core skills use direct Rust trait calls (no IPC).
/// Community/sideloaded skills will use IPC instead.
pub trait CoreSkill: Send + Sync {
    fn name(&self) -> &str;
    fn activate(&mut self) -> anyhow::Result<()>;
    fn deactivate(&mut self) -> anyhow::Result<()>;
    fn handles(&self, file_type: &str) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockSkill {
        active: bool,
    }

    impl CoreSkill for MockSkill {
        fn name(&self) -> &str {
            "mock-skill"
        }
        fn activate(&mut self) -> anyhow::Result<()> {
            self.active = true;
            Ok(())
        }
        fn deactivate(&mut self) -> anyhow::Result<()> {
            self.active = false;
            Ok(())
        }
        fn handles(&self, file_type: &str) -> bool {
            file_type == "md"
        }
    }

    #[test]
    fn test_core_skill_trait_implementable() {
        let mut skill = MockSkill { active: false };
        assert_eq!(skill.name(), "mock-skill");
        assert!(!skill.active);

        skill.activate().unwrap();
        assert!(skill.active);
        assert!(skill.handles("md"));
        assert!(!skill.handles("pdf"));

        skill.deactivate().unwrap();
        assert!(!skill.active);
    }

    #[test]
    fn test_core_skill_is_object_safe() {
        let skill: Box<dyn CoreSkill> = Box::new(MockSkill { active: false });
        assert_eq!(skill.name(), "mock-skill");
    }
}
