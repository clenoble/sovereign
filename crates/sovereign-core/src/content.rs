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

/// How a stored content string was read by [`ContentFields::parse_with_kind`].
///
/// Writers disagree on the stored form (the envelope vs a bare body — open
/// decision K1), so readers must be tolerant. Callers that are about to
/// *overwrite* a document use the kind to refuse an overwrite when the read
/// fell back to raw text (the stored content was not an envelope).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentKind {
    /// The input was the JSON envelope `{"body": …, "images": …, "videos": …}`.
    Envelope,
    /// The input was not an envelope; the whole string became the body.
    Raw,
    /// The input was the empty string.
    Empty,
}

impl ContentFields {
    /// Tolerant read of a stored content string.
    ///
    /// A valid envelope parses as before. Anything else — markdown, plain
    /// text, JSON that is not an envelope — becomes a bare body holding the
    /// whole input, with no media. A non-empty input never yields an empty
    /// body unless it is an envelope whose `body` is empty.
    pub fn parse(content: &str) -> Self {
        Self::parse_with_kind(content).0
    }

    /// Like [`parse`](Self::parse), but also says whether the input was an
    /// envelope or fell back to raw text.
    ///
    /// An envelope is a JSON object carrying a string `body` field that
    /// deserializes into `ContentFields` (unknown extra fields are tolerated;
    /// `images`/`videos` default to empty). Every writer of the envelope emits
    /// `body`, so requiring it only reclassifies JSON objects that were never
    /// envelopes (`{}`, `{"name": …}`) — which used to read as empty.
    pub fn parse_with_kind(content: &str) -> (Self, ContentKind) {
        if content.is_empty() {
            return (Self::default(), ContentKind::Empty);
        }
        if let Ok(serde_json::Value::Object(map)) =
            serde_json::from_str::<serde_json::Value>(content)
        {
            if map.get("body").is_some_and(|b| b.is_string()) {
                match serde_json::from_value::<ContentFields>(serde_json::Value::Object(map)) {
                    Ok(cf) => return (cf, ContentKind::Envelope),
                    Err(e) => tracing::warn!(
                        "content looks like an envelope but does not parse ({e}); reading it as raw text"
                    ),
                }
            }
        }
        (
            Self {
                body: content.to_string(),
                ..Self::default()
            },
            ContentKind::Raw,
        )
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
    fn parse_empty_is_empty() {
        let (cf, kind) = ContentFields::parse_with_kind("");
        assert_eq!(cf.body, "");
        assert!(cf.images.is_empty());
        assert!(cf.videos.is_empty());
        assert_eq!(kind, ContentKind::Empty);
    }

    /// Invariant: a non-empty input that is not an envelope is kept whole as
    /// the body — never emptied, never truncated, no media invented.
    #[test]
    fn non_envelope_input_is_kept_whole() {
        let samples = [
            "not json",
            "# Title\n\nSome *markdown* body.",
            "   ",
            "{",
            "{}",
            r#"{"name": "x"}"#,
            r#"{"body": 42}"#,
            r#"{"body": null, "images": []}"#,
            r#"{"body": "x", "images": "not a list"}"#,
            "[1, 2, 3]",
            r#""a json string""#,
            "123",
            "null",
            r#"{"body":"truncated"#,
        ];
        for s in samples {
            let (cf, kind) = ContentFields::parse_with_kind(s);
            assert_eq!(cf.body, s, "input {s:?} must survive as the body");
            assert!(cf.images.is_empty() && cf.videos.is_empty(), "{s:?}");
            assert_eq!(kind, ContentKind::Raw, "{s:?}");
            assert_eq!(ContentFields::parse(s).body, s);
        }
    }

    /// Invariant: parse never turns a non-empty input into an empty body,
    /// except an envelope whose body really is empty.
    #[test]
    fn non_empty_input_never_reads_as_empty_body_unless_envelope_says_so() {
        let samples = [
            "x",
            "{}",
            r#"{"images": [{"path": "/a.png"}]}"#,
            r#"{"body":"","images":[]}"#,
            r#"{"body":"hello"}"#,
        ];
        for s in samples {
            let (cf, kind) = ContentFields::parse_with_kind(s);
            if cf.body.is_empty() {
                assert_eq!(kind, ContentKind::Envelope, "{s:?} read as empty without being an envelope");
            }
        }
    }

    #[test]
    fn valid_envelopes_parse_as_envelope() {
        for s in [
            r#"{"body":"","images":[]}"#, // Document::new
            r#"{"body":"old doc","images":[]}"#,
            r#"{"body":"x","images":[],"videos":[],"extra":1}"#,
            r#"{"body":"x"}"#,
        ] {
            assert_eq!(ContentFields::parse_with_kind(s).1, ContentKind::Envelope, "{s:?}");
        }
    }

    /// Invariant: a serialized envelope round-trips unchanged.
    #[test]
    fn envelope_round_trips_unchanged() {
        let cf = ContentFields {
            body: "{\"body\": \"nested json-looking text\"}\n".into(),
            images: vec![ContentImage { path: "/i.png".into(), caption: "c".into() }],
            videos: vec![],
        };
        let s = cf.serialize();
        let (back, kind) = ContentFields::parse_with_kind(&s);
        assert_eq!(kind, ContentKind::Envelope);
        assert_eq!(back.serialize(), s);
        assert_eq!(back.body, cf.body);
        assert_eq!(back.images, cf.images);
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
