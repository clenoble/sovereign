use crate::traits::{CoreSkill, SkillDocument, SkillOutput};
use sovereign_core::content::{ContentFields, ContentVideo};

/// Skill for managing video references in documents.
///
/// Videos are stored as file path references in ContentFields.
/// Playback delegates to the system's default media player.
pub struct VideoSkill;

impl CoreSkill for VideoSkill {
    fn name(&self) -> &str {
        "video"
    }

    fn activate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn execute(
        &self,
        action: &str,
        doc: &SkillDocument,
        params: &str,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "add" => {
                if params.is_empty() {
                    anyhow::bail!("Video path required");
                }
                let path = std::path::Path::new(params);
                let caption = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let video = ContentVideo {
                    path: params.to_string(),
                    caption,
                    duration_secs: None,
                    thumbnail_path: None,
                };

                let mut videos = doc.content.videos.clone();
                videos.push(video);

                Ok(SkillOutput::ContentUpdate(ContentFields {
                    body: doc.content.body.clone(),
                    images: doc.content.images.clone(),
                    videos,
                }))
            }
            "remove" => {
                let idx: usize = params
                    .parse()
                    .map_err(|_| anyhow::anyhow!("Invalid video index: {params}"))?;
                if idx >= doc.content.videos.len() {
                    anyhow::bail!(
                        "Video index {idx} out of range (have {})",
                        doc.content.videos.len()
                    );
                }
                let mut videos = doc.content.videos.clone();
                videos.remove(idx);

                Ok(SkillOutput::ContentUpdate(ContentFields {
                    body: doc.content.body.clone(),
                    images: doc.content.images.clone(),
                    videos,
                }))
            }
            "play" => {
                // Playback is handled in the UI layer via open::that().
                // This action exists for skill registry completeness.
                Ok(SkillOutput::None)
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![
            ("add".into(), "Add Video".into()),
            ("remove".into(), "Remove Video".into()),
            ("play".into(), "Play Video".into()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentFields;

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test".into(),
            content: ContentFields {
                body: "text".into(),
                images: vec![],
                videos: vec![],
            },
        }
    }

    fn make_doc_with_video() -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test".into(),
            content: ContentFields {
                body: "text".into(),
                images: vec![],
                videos: vec![
                    ContentVideo {
                        path: "/tmp/a.mp4".into(),
                        caption: "First".into(),
                        duration_secs: Some(60.0),
                        thumbnail_path: None,
                    },
                    ContentVideo {
                        path: "/tmp/b.mp4".into(),
                        caption: "Second".into(),
                        duration_secs: None,
                        thumbnail_path: None,
                    },
                ],
            },
        }
    }

    #[test]
    fn add_video() {
        let skill = VideoSkill;
        let doc = make_doc();
        let result = skill.execute("add", &doc, "/path/to/demo.mp4").unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert_eq!(cf.videos.len(), 1);
                assert_eq!(cf.videos[0].path, "/path/to/demo.mp4");
                assert_eq!(cf.videos[0].caption, "demo");
                assert_eq!(cf.body, "text");
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn add_video_empty_path_fails() {
        let skill = VideoSkill;
        let doc = make_doc();
        assert!(skill.execute("add", &doc, "").is_err());
    }

    #[test]
    fn remove_video() {
        let skill = VideoSkill;
        let doc = make_doc_with_video();
        let result = skill.execute("remove", &doc, "0").unwrap();
        match result {
            SkillOutput::ContentUpdate(cf) => {
                assert_eq!(cf.videos.len(), 1);
                assert_eq!(cf.videos[0].path, "/tmp/b.mp4");
            }
            _ => panic!("Expected ContentUpdate"),
        }
    }

    #[test]
    fn remove_video_out_of_range() {
        let skill = VideoSkill;
        let doc = make_doc();
        assert!(skill.execute("remove", &doc, "0").is_err());
    }

    #[test]
    fn play_returns_none() {
        let skill = VideoSkill;
        let doc = make_doc();
        let result = skill.execute("play", &doc, "").unwrap();
        assert!(matches!(result, SkillOutput::None));
    }

    #[test]
    fn unknown_action_fails() {
        let skill = VideoSkill;
        let doc = make_doc();
        assert!(skill.execute("unknown", &doc, "").is_err());
    }

    #[test]
    fn actions_list() {
        let skill = VideoSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 3);
    }
}
