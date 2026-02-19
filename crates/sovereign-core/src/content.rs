use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ContentFields {
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub images: Vec<ContentImage>,
    #[serde(default)]
    pub videos: Vec<ContentVideo>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ContentImage {
    pub path: String,
    #[serde(default)]
    pub caption: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ContentVideo {
    pub path: String,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub duration_secs: Option<f64>,
    #[serde(default)]
    pub thumbnail_path: Option<String>,
}

impl ContentFields {
    pub fn parse(json: &str) -> Self {
        match serde_json::from_str(json) {
            Ok(cf) => cf,
            Err(e) => {
                if !json.is_empty() {
                    tracing::warn!("ContentFields parse failed (falling back to default): {e}");
                }
                Self::default()
            }
        }
    }

    pub fn serialize(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_json() {
        let json = r##"{"body": "Hello World", "images": [{"path": "/tmp/a.png", "caption": "test"}]}"##;
        let cf = ContentFields::parse(json);
        assert_eq!(cf.body, "Hello World");
        assert_eq!(cf.images.len(), 1);
        assert_eq!(cf.images[0].path, "/tmp/a.png");
        assert_eq!(cf.images[0].caption, "test");
        assert!(cf.videos.is_empty());
    }

    #[test]
    fn parse_empty_or_invalid_returns_default() {
        let cf = ContentFields::parse("");
        assert_eq!(cf.body, "");
        assert!(cf.images.is_empty());
        assert!(cf.videos.is_empty());

        let cf = ContentFields::parse("not json");
        assert_eq!(cf.body, "");
        assert!(cf.images.is_empty());
    }

    #[test]
    fn serialize_roundtrip() {
        let cf = ContentFields {
            body: "Hello".to_string(),
            images: vec![ContentImage {
                path: "/tmp/img.png".to_string(),
                caption: "Cap".to_string(),
            }],
            videos: vec![ContentVideo {
                path: "/tmp/vid.mp4".to_string(),
                caption: "Demo".to_string(),
                duration_secs: Some(120.5),
                thumbnail_path: None,
            }],
        };
        let json = cf.serialize();
        let cf2 = ContentFields::parse(&json);
        assert_eq!(cf2.body, "Hello");
        assert_eq!(cf2.images.len(), 1);
        assert_eq!(cf2.images[0].path, "/tmp/img.png");
        assert_eq!(cf2.videos.len(), 1);
        assert_eq!(cf2.videos[0].path, "/tmp/vid.mp4");
        assert_eq!(cf2.videos[0].caption, "Demo");
        assert_eq!(cf2.videos[0].duration_secs, Some(120.5));
    }

    #[test]
    fn backward_compatible_without_videos() {
        let json = r#"{"body":"old doc","images":[]}"#;
        let cf = ContentFields::parse(json);
        assert_eq!(cf.body, "old doc");
        assert!(cf.videos.is_empty());
    }
}
