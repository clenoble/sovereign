//! Helpers for producing `ContentFields` outputs from skills.
//!
//! Skills that return `SkillOutput::ContentUpdate` must construct a
//! `ContentFields` value carrying the modified body alongside the
//! original images and videos. This is mechanical and identical across
//! every body-rewriting skill — extracted here.

use sovereign_core::content::ContentFields;

use crate::traits::SkillDocument;

/// Build a `ContentFields` that replaces only the body, preserving the
/// document's images and videos verbatim. Use as the payload for
/// `SkillOutput::ContentUpdate`.
pub fn replace_body(doc: &SkillDocument, body: String) -> ContentFields {
    ContentFields {
        body,
        images: doc.content.images.clone(),
        videos: doc.content.videos.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::ContentImage;

    #[test]
    fn replace_body_swaps_body_keeps_attachments() {
        let doc = SkillDocument {
            id: "document:t".into(),
            title: "T".into(),
            content: ContentFields {
                body: "old".into(),
                images: vec![ContentImage { path: "img.png".into(), caption: None }],
                videos: vec![],
            },
        };
        let new = replace_body(&doc, "new".into());
        assert_eq!(new.body, "new");
        assert_eq!(new.images.len(), 1);
        assert_eq!(new.images[0].path, "img.png");
    }
}
