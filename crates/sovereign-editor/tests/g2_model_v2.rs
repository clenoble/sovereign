//! Guarantee 2 at the level of the (PROVISIONAL) v2 model the editor holds, and
//! the reading of older forms (spec ch. 03, *Document format v2 — blocks*).

mod common;
use common::*;

use sovereign_editor::doc::{DocumentV2, SourceForm};

fn roundtrip(s: &str) -> String {
    let (doc, form) = DocumentV2::read(s).expect("readable");
    assert_eq!(form, SourceForm::V2);
    doc.to_json_string()
}

// "Writing a document whose blocks did not change produces the same bytes: key
// order and number formatting are preserved for unchanged blocks."
#[test]
fn g2_model_unchanged_v2_round_trips_byte_for_byte() {
    let cases = [
        r#"{"format":"sovereign.doc","v":2,"blocks":[]}"#.to_string(),
        doc1("plain"),
        v2(&[unknown_block(1), text_block(2, "a\r\nb"), unknown_block(3)]),
        // odd key order, escapes kept as written
        format!(r#"{{"blocks":[{{"text":"caf\u00e9 \/ \"q\"","markup":"markdown","id":"{}","type":"text"}}],"v":2,"format":"sovereign.doc"}}"#, id(1)),
        // numbers that a float round trip would rewrite
        format!(r#"{{"format":"sovereign.doc","v":2,"blocks":[{{"type":"x","id":"{}","n":[1.0,1.50,1e3,1E-7,-0,-0.0,100000000000000000000000000001,0.1000000000000000055511151231257827]}}]}}"#, id(9)),
        // non-ASCII written raw, astral characters, a lone escaped surrogate pair
        doc1("👩‍👩‍👧‍👦 مرحبا \u{1F1EB}\u{1F1F7}"),
        format!(r#"{{"format":"sovereign.doc","v":2,"blocks":[{{"type":"text","id":"{}","markup":"markdown","text":"\ud83d\ude00"}}]}}"#, id(1)),
    ];
    for c in cases {
        assert_eq!(roundtrip(&c), c);
    }
}

// Same, read strictly: "produces the same bytes" for a document whose
// formatting (whitespace, indentation) Sovereign did not write itself.
#[test]
fn g2_model_unchanged_pretty_printed_v2_round_trips_byte_for_byte() {
    let c = format!(
        "{{\n  \"format\": \"sovereign.doc\",\n  \"v\": 2,\n  \"blocks\": [\n    {}\n  ]\n}}\n",
        text_block(1, "pretty")
    );
    assert_eq!(roundtrip(&c), c);
}

// "Unknown block types are preserved exactly — same position, same JSON value"
#[test]
fn g2_model_unknown_blocks_are_read_as_unknown_and_kept() {
    let s = v2(&[unknown_block(1), text_block(2, "t"), unknown_block(3)]);
    let (doc, _) = DocumentV2::read(&s).unwrap();
    let blocks = doc.blocks();
    assert_eq!(blocks.len(), 3);
    assert!(!blocks[0].is_known());
    assert_eq!(blocks[0].type_name(), "x-chart-v9");
    assert_eq!(blocks[0].id(), bid(1));
    assert!(blocks[0].text().is_none(), "an unknown block's \"text\" field is not editable text");
    assert!(blocks[1].is_known());
    assert_eq!(blocks[1].text(), Some("t"));
    assert_eq!(blocks[1].markup(), Some("markdown"));
    assert!(!blocks[2].is_known());
}

// "A raw-text body → one text block holding the whole string." (ch. 03)
#[test]
fn g2_model_raw_text_reads_as_one_text_block_holding_the_whole_string() {
    let cases = [
        "plain",
        "# Title\r\n\r\nbody with [pii:pii_record:3] and \u{0} nul",
        "{not json",
        r#"{"name":"a JSON object that is not an envelope"}"#,
        "[1,2,3]",
        "  leading and trailing spaces  ",
    ];
    for raw in cases {
        let (doc, form) = DocumentV2::read(raw).unwrap();
        assert_eq!(form, SourceForm::RawText, "{raw:?}");
        assert_eq!(doc.blocks().len(), 1, "{raw:?}");
        assert_eq!(doc.blocks()[0].type_name(), "text");
        assert_eq!(doc.blocks()[0].text(), Some(raw));
    }
}

// "The v1 envelope {"body", "images", "videos"} → one text block holding body,
// then one image / video block per entry, in that order." (ch. 03)
#[test]
fn g2_model_v1_envelope_migrates_in_order() {
    let v1 = r#"{"body":"hello **v1**","images":[{"path":"/a.png","caption":"A"},{"path":"/b.png","caption":"B"}],"videos":[{"path":"/c.mp4","caption":"C","duration_secs":3.5,"thumbnail_path":"/c.jpg"}]}"#;
    let (doc, form) = DocumentV2::read(v1).unwrap();
    assert_eq!(form, SourceForm::V1Envelope);
    let types: Vec<&str> = doc.blocks().iter().map(|b| b.type_name()).collect();
    assert_eq!(types, ["text", "image", "image", "video"]);
    assert_eq!(doc.blocks()[0].text(), Some("hello **v1**"));
    let out = parse(&doc.to_json_string());
    assert_eq!(out["format"], "sovereign.doc");
    assert_eq!(out["v"], 2);
    let b = out["blocks"].as_array().unwrap();
    assert_eq!(b[1]["ref"]["path"], "/a.png");
    assert_eq!(b[1]["caption"], "A");
    assert_eq!(b[2]["ref"]["path"], "/b.png");
    assert_eq!(b[3]["ref"]["path"], "/c.mp4");
    assert_eq!(b[3]["caption"], "C");
    let mut ids: Vec<&str> = b.iter().map(|x| x["id"].as_str().unwrap()).collect();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 4, "every migrated block gets its own id");
}

// Decided by Céline 2026-10-10: "migration produces image blocks with `alt`
// empty and a 'needs a description' flag, and `ref.sha256` absent until the
// document service (host) computes it; the editor never reads files … an image
// block from v1 migration is valid with those two fields pending."
#[test]
fn g2_model_v1_media_migrate_with_description_and_hash_pending() {
    let v1 = r#"{"body":"b","images":[{"path":"/a.png","caption":"A"}],"videos":[{"path":"/c.mp4","caption":"C"}]}"#;
    let (doc, _) = DocumentV2::read(v1).unwrap();
    let img = &doc.blocks()[1];
    assert!(img.is_known(), "a migrated image is a valid, known block");
    assert_eq!(img.media_path(), Some("/a.png"));
    assert_eq!(img.image_alt(), Some(""));
    assert!(img.needs_description());
    assert_eq!(img.media_sha256(), None, "the hash is the host's to compute");
    let vid = &doc.blocks()[2];
    assert!(vid.is_known());
    assert_eq!(vid.media_path(), Some("/c.mp4"));
    assert_eq!(vid.media_sha256(), None);
    let out = parse(&doc.to_json_string());
    assert_eq!(out["blocks"][1]["alt"], "");
    assert!(out["blocks"][1]["ref"].get("sha256").is_none(), "no placeholder hash is written");
    assert!(out["blocks"][2]["ref"].get("sha256").is_none());
    // the pending state survives being written and read back
    let (again, form) = DocumentV2::read(&doc.to_json_string()).unwrap();
    assert_eq!(form, SourceForm::V2);
    assert!(again.blocks()[1].is_known());
    assert!(again.blocks()[1].needs_description());
    assert_eq!(again.blocks()[1].media_sha256(), None);
}

// The flag is not set on an image that has its description, and a hash that is
// there is read.
#[test]
fn g2_model_described_image_does_not_need_a_description() {
    let s = v2(&[format!(
        r#"{{"type":"image","id":"{}","ref":{{"path":"/p.png","sha256":"ab12"}},"alt":"a cat","caption":""}}"#,
        id(1)
    )]);
    let (doc, _) = DocumentV2::read(&s).unwrap();
    assert!(!doc.blocks()[0].needs_description());
    assert_eq!(doc.blocks()[0].image_alt(), Some("a cat"));
    assert_eq!(doc.blocks()[0].media_sha256(), Some("ab12"));
}

// "A reader that meets a v higher than it knows opens the document read-only
// and never rewrites it." — the model keeps the input as it was.
#[test]
fn g2_model_newer_version_is_reported_and_kept_verbatim() {
    let s = r#"{"format":"sovereign.doc","v":7,"blocks":[{"type":"text","id":"00","text":"x"}],"z":[1.50]}"#;
    let (doc, form) = DocumentV2::read(s).unwrap();
    assert_eq!(form, SourceForm::NewerVersion { v: 7 });
    assert_eq!(doc.to_json_string(), s);
}

// Guarantee 9's quantity: text bytes are the UTF-8 bytes of text blocks only.
#[test]
fn g2_model_text_bytes_counts_text_blocks_only() {
    let s = v2(&[text_block(1, "héllo"), unknown_block(2), text_block(3, "👍")]);
    let (doc, _) = DocumentV2::read(&s).unwrap();
    assert_eq!(doc.text_bytes(), "héllo".len() + "👍".len());
}

// Block ids: the stored hex form round trips.
#[test]
fn g2_model_block_id_hex_round_trip() {
    let h = id(42);
    assert!(sovereign_editor::doc::BlockId::from_hex(&h).unwrap().to_hex().eq_ignore_ascii_case(&h));
    assert!(sovereign_editor::doc::BlockId::from_hex("xyz").is_none());
}
