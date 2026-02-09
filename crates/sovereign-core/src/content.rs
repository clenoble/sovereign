use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Clone, Debug)]
pub struct ContentFields {
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub images: Vec<ContentImage>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContentImage {
    pub path: String,
    #[serde(default)]
    pub caption: String,
}

impl ContentFields {
    pub fn parse(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
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
    }

    #[test]
    fn parse_empty_or_invalid_returns_default() {
        let cf = ContentFields::parse("");
        assert_eq!(cf.body, "");
        assert!(cf.images.is_empty());

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
        };
        let json = cf.serialize();
        let cf2 = ContentFields::parse(&json);
        assert_eq!(cf2.body, "Hello");
        assert_eq!(cf2.images.len(), 1);
        assert_eq!(cf2.images[0].path, "/tmp/img.png");
    }
}
