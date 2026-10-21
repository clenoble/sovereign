use serde::{Deserialize, Serialize};
use sovereign_docformat::{DocumentV2, SourceForm};

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

/// A stored content string read through the v2 model, for a reader that still
/// hands out v1 fields (`ContentFields`): the Tauri document DTO and the skill
/// input (C-DOC adoption step 1; the writers that take these fields back are
/// step 2 and C-SKILL).
pub struct FieldsRead {
    /// The text blocks as one body ([`DocumentV2::plain_text`]), the image and
    /// video blocks as v1 media.
    pub fields: ContentFields,
    /// The stored form found; `None` when the content claims to be a Sovereign
    /// document but cannot be read (then `fields` is empty).
    pub form: Option<SourceForm>,
    /// Whether writing `fields` back as a v1 envelope would keep everything
    /// the stored content holds: true for the empty string, raw text (the
    /// envelope's body holds it whole) and a v1 envelope whose entries all read
    /// as media; false for v2, a newer format, an unreadable document, and a
    /// v1 envelope with an entry that is not a readable image or video. A
    /// writer that still writes envelopes must refuse when this is false.
    pub envelope_round_trip: bool,
}

impl ContentFields {
    /// The v1 fields of a v2 document: the text blocks joined by one blank
    /// line as the body (tokens kept as `[pii:<id>]`), image blocks as images
    /// (path, caption), video blocks as videos (path, caption, duration,
    /// thumbnail). Tables and unknown blocks have no v1 field.
    pub fn from_document(doc: &DocumentV2) -> Self {
        let mut fields = ContentFields { body: doc.plain_text(), ..Self::default() };
        for b in doc.blocks() {
            let Some(path) = b.media_path() else { continue };
            let caption = b.caption().unwrap_or_default().to_string();
            match b.type_name() {
                "image" => fields.images.push(ContentImage { path: path.to_string(), caption }),
                "video" => fields.videos.push(ContentVideo {
                    path: path.to_string(),
                    caption,
                    duration_secs: b.video_duration_secs(),
                    thumbnail_path: b.video_thumbnail_path(),
                }),
                _ => {}
            }
        }
        fields
    }

    /// Reads a stored content string in any form (v2, v1 envelope, raw text,
    /// empty) through [`DocumentV2::read`]; see [`FieldsRead`].
    pub fn read_stored(stored: &str) -> FieldsRead {
        match DocumentV2::read(stored) {
            Ok((doc, form)) => {
                let all_known = doc
                    .blocks()
                    .iter()
                    .all(|b| b.is_known() && matches!(b.type_name(), "text" | "image" | "video"));
                let envelope_round_trip = match form {
                    SourceForm::Empty | SourceForm::RawText => true,
                    SourceForm::V1Envelope => all_known,
                    SourceForm::V2 | SourceForm::NewerVersion { .. } => false,
                };
                FieldsRead { fields: Self::from_document(&doc), form: Some(form), envelope_round_trip }
            }
            Err(_) => FieldsRead { fields: Self::default(), form: None, envelope_round_trip: false },
        }
    }

    pub fn serialize(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V2: &str = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000000a01","text":"first [pii:pii_record:e1]"},{"type":"image","id":"00000000000000000000000000000a02","ref":{"path":"/i.png"},"alt":"a","caption":"cap"},{"type":"text","id":"00000000000000000000000000000a03","text":"second"}]}"#;

    /// Adoption step 1: the three stored forms read to their v1 fields (the
    /// body of an envelope, raw text whole), and v2 reads as text, not JSON.
    #[test]
    fn read_stored_reads_v2_v1_and_raw() {
        let r = ContentFields::read_stored(V2);
        assert_eq!(r.form, Some(SourceForm::V2));
        assert_eq!(r.fields.body, "first [pii:pii_record:e1]\n\nsecond");
        assert_eq!(r.fields.images, vec![ContentImage { path: "/i.png".into(), caption: "cap".into() }]);
        assert!(!r.envelope_round_trip, "a v2 document must not be written back as an envelope");

        let v1 = ContentFields {
            body: "env [pii:pii_record:e1]".into(),
            images: vec![ContentImage { path: "/a.png".into(), caption: "c".into() }],
            videos: vec![ContentVideo {
                path: "/v.mp4".into(),
                caption: "v".into(),
                duration_secs: Some(3.5),
                thumbnail_path: Some("/t.png".into()),
            }],
        }
        .serialize();
        let r = ContentFields::read_stored(&v1);
        assert_eq!(r.form, Some(SourceForm::V1Envelope));
        assert!(r.envelope_round_trip);
        assert_eq!(r.fields.serialize(), v1, "a v1 envelope round-trips unchanged");

        for raw in ["plain text", "{}", r#"{"name":"x"}"#, "[1,2]"] {
            let r = ContentFields::read_stored(raw);
            assert_eq!(r.form, Some(SourceForm::RawText), "{raw:?}");
            assert_eq!(r.fields.body, raw);
            assert!(r.envelope_round_trip);
        }

        let r = ContentFields::read_stored("");
        assert_eq!(r.form, Some(SourceForm::Empty));
        assert!(r.envelope_round_trip && r.fields.body.is_empty());

        let r = ContentFields::read_stored(r#"{"format":"sovereign.doc","blocks":[]}"#);
        assert_eq!(r.form, None);
        assert!(!r.envelope_round_trip && r.fields.body.is_empty());

        // A v1 entry that is not a readable image is kept by the read, but has
        // no v1 field: writing the envelope back would drop it.
        let r = ContentFields::read_stored(r#"{"body":"b","images":[42]}"#);
        assert_eq!(r.form, Some(SourceForm::V1Envelope));
        assert!(!r.envelope_round_trip);
    }

    #[test]
    fn an_envelope_without_videos_reads() {
        let r = ContentFields::read_stored(r#"{"body":"old doc","images":[]}"#);
        assert_eq!(r.form, Some(SourceForm::V1Envelope));
        assert_eq!(r.fields.body, "old doc");
        assert!(r.fields.videos.is_empty());
    }

    /// The v1 serialiser (kept for test fixtures and the readers of v1
    /// fields): what it writes reads back unchanged.
    #[test]
    fn serialize_round_trips_through_the_reader() {
        let cf = ContentFields {
            body: "{\"body\": \"nested json-looking text\"}
".into(),
            images: vec![ContentImage { path: "/tmp/img.png".into(), caption: "Cap".into() }],
            videos: vec![ContentVideo {
                path: "/tmp/vid.mp4".into(),
                caption: "Demo".into(),
                duration_secs: Some(120.5),
                thumbnail_path: None,
            }],
        };
        let json = cf.serialize();
        let r = ContentFields::read_stored(&json);
        assert_eq!(r.form, Some(SourceForm::V1Envelope));
        assert_eq!(r.fields.body, cf.body);
        assert_eq!(r.fields.images, cf.images);
        assert_eq!(r.fields.videos, cf.videos);
        assert_eq!(r.fields.serialize(), json);
    }
}
