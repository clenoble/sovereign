//! The media store — a medium must be a medium, by the list of formats.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*, *The media
//! store*, v2.0.81 (Céline, 2026-11-13: « Copier, avec les garde-fous »): "A
//! medium must be a medium: the pass, and insertion too, copy a file only if
//! its first bytes say image or video, never by its name. Anything else is
//! neutralised, counted and said."
//!
//! v2.0.84 (Céline, 2026-11-14: « Liste large + registre », and for SVG « Les
//! accepter »): "The formats are a list in the contract, broad. Images: PNG,
//! JPEG, GIF, WebP, BMP, TIFF, HEIC and HEIF, AVIF, and SVG. Videos: MP4 and
//! MOV, WebM, AVI. Each is known by the signature its format defines, and each
//! has its test."
//!
//! v2.0.84 final: "A format with a weak signature gets a check as strong as the
//! guard's purpose. [...] An SVG is a file whose root element is `svg`. It is
//! read within a stated bound larger than the binary sniff. A declaration, a
//! DOCTYPE, comments and white space may come before it; nothing else may. An
//! XML file with another root is not a medium, whatever its name. A BMP is
//! accepted on a coherent header, not on its two letters « BM »."
//!
//! `is_medium` takes the first bytes only: the name cannot enter the decision,
//! so "whatever its name" holds by its signature (the service side, a file
//! named `.svg`, is in `media_store_copy.rs`).
//! Each test feeds the minimal header that its format's signature defines,
//! within `MEDIUM_SNIFF_BYTES`. The service side (insertion, the pass) is in
//! `media_store_copy.rs`. Showing an SVG inert is a rendering requirement, not
//! a sniffing one: it is not tested here (see the lot's report).

use sovereign_docs::{is_medium, MEDIUM_SNIFF_BYTES, SVG_SNIFF_BYTES};

fn accepted(what: &str, head: &[u8]) {
    assert!(head.len() <= MEDIUM_SNIFF_BYTES, "{what}: the test header fits in the binary sniff");
    assert!(is_medium(head), "{what} is a medium, by its signature");
}

fn accepted_text(what: &str, head: &[u8]) {
    assert!(head.len() <= SVG_SNIFF_BYTES, "{what}: the test text fits in the SVG bound");
    assert!(is_medium(head), "{what} is a medium, by its root element");
}

fn refused(what: &str, head: &[u8]) {
    assert!(!is_medium(head), "{what} is not a medium");
}

// ---------------------------------------------------------------- images --

// The list (v2.0.84): PNG, by its eight-byte signature.
#[test]
fn store_format_png_is_a_medium() {
    accepted("PNG", b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR");
}

// The list (v2.0.84): JPEG, by its start-of-image marker.
#[test]
fn store_format_jpeg_is_a_medium() {
    accepted("JPEG (JFIF)", b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01");
    accepted("JPEG (Exif)", b"\xff\xd8\xff\xe1\x00\x18Exif\x00\x00");
}

// The list (v2.0.84): GIF, both versions.
#[test]
fn store_format_gif_is_a_medium() {
    accepted("GIF89a", b"GIF89a\x01\x00\x01\x00\x80\x00\x00");
    accepted("GIF87a", b"GIF87a\x01\x00\x01\x00\x80\x00\x00");
}

// The list (v2.0.84): WebP, a RIFF container of form WEBP.
#[test]
fn store_format_webp_is_a_medium() {
    accepted("WebP", b"RIFF\x24\x00\x00\x00WEBPVP8 \x18\x00\x00\x00");
}

// The list (v2.0.84): BMP, on a coherent header (file header, then a DIB header of a known size).
#[test]
fn store_format_bmp_is_a_medium() {
    accepted("BMP", b"BM\x46\x00\x00\x00\x00\x00\x00\x00\x36\x00\x00\x00\x28\x00\x00\x00");
}

// The list (v2.0.84): TIFF, both byte orders.
#[test]
fn store_format_tiff_is_a_medium() {
    accepted("TIFF little-endian", b"II*\x00\x08\x00\x00\x00");
    accepted("TIFF big-endian", b"MM\x00*\x00\x00\x00\x08");
}

// The list (v2.0.84): HEIC and HEIF, an ISO-BMFF ftyp box with their brands.
#[test]
fn store_format_heic_and_heif_are_media() {
    accepted("HEIC", b"\x00\x00\x00\x18ftypheic\x00\x00\x00\x00mif1heic");
    accepted("HEIF", b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00mif1heic");
}

// The list (v2.0.84): AVIF, an ISO-BMFF ftyp box of brand avif.
#[test]
fn store_format_avif_is_a_medium() {
    accepted("AVIF", b"\x00\x00\x00\x1cftypavif\x00\x00\x00\x00avifmif1miaf");
}

// The list (v2.0.84): SVG is accepted (Céline: « Les accepter »), by its root element, with or without an XML declaration.
#[test]
fn store_format_svg_is_a_medium() {
    accepted_text("SVG with an XML declaration", b"<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\">");
    accepted_text("SVG", b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\">");
}

// v2.0.84: an SVG whose root element comes after a long prolog (declaration, comment, DOCTYPE, white space), past the binary sniff, is a medium.
#[test]
fn store_format_svg_after_a_long_prolog_is_a_medium() {
    let svg: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?>\n\
<!-- Created with an editor that writes a long comment before the drawing itself -->\n\
<!DOCTYPE svg PUBLIC \"-//W3C//DTD SVG 1.1//EN\" \"http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd\">\n\
\n   <svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1\" height=\"1\"></svg>\n";
    let at = svg.windows(4).position(|w| w == b"<svg").unwrap();
    assert!(at > MEDIUM_SNIFF_BYTES, "precondition: `<svg` starts past the binary sniff ({at} bytes in)");
    accepted_text("SVG after a long prolog", svg);
}

// ---------------------------------------------------------------- videos --

// The list (v2.0.84): MP4, an ISO-BMFF ftyp box of an MP4 brand.
#[test]
fn store_format_mp4_is_a_medium() {
    accepted("MP4 (isom)", b"\x00\x00\x00\x18ftypisom\x00\x00\x02\x00isomiso2");
    accepted("MP4 (mp42)", b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom");
}

// The list (v2.0.84): MOV, an ftyp box of brand "qt  ".
#[test]
fn store_format_mov_is_a_medium() {
    accepted("MOV", b"\x00\x00\x00\x14ftypqt  \x00\x00\x02\x00qt  ");
}

// The list (v2.0.84): WebM, an EBML header of DocType webm.
#[test]
fn store_format_webm_is_a_medium() {
    accepted(
        "WebM",
        b"\x1a\x45\xdf\xa3\x9f\x42\x86\x81\x01\x42\xf7\x81\x01\x42\xf2\x81\x04\x42\xf3\x81\x08\x42\x82\x84webm",
    );
}

// The list (v2.0.84): AVI, a RIFF container of form "AVI ".
#[test]
fn store_format_avi_is_a_medium() {
    accepted("AVI", b"RIFF\x24\x00\x00\x00AVI LIST\x00\x00\x00\x00");
}

// ------------------------------------------------------------- not media --

// A medium must be a medium: a text file renamed .png, or anything else outside the list, is not.
#[test]
fn store_a_file_that_is_not_an_image_or_a_video_is_not_a_medium() {
    refused("text renamed .png", b"These are my notes, not a picture.\n");
    refused("empty", b"");
    refused("PDF", b"%PDF-1.7\n%\xe2\xe3\xcf\xd3");
    refused("Windows executable", b"MZ\x90\x00\x03\x00\x00\x00");
    refused("ssh key", b"-----BEGIN OPENSSH PRIVATE KEY-----\n");
}

// v2.0.84: "An XML file with another root is not a medium, whatever its name" — a password list, a document, an XML file whose root only mentions svg.
#[test]
fn store_an_xml_file_with_another_root_is_not_a_medium() {
    refused("XML password list", b"<?xml version=\"1.0\"?>\n<passwords><entry site=\"bank\">hunter2</entry></passwords>");
    refused("XML with a comment, then another root", b"<?xml version=\"1.0\"?>\n<!-- svg -->\n<keys><svg/></keys>");
    refused("text, then an svg element", b"Notes about the logo:\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>");
    refused("KeePass XML export", b"<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"yes\"?>\n<KeePassFile><Meta>");
}

// v2.0.84: "A BMP is accepted on a coherent header, not on its two letters « BM »." A text file beginning with BM is not a medium.
#[test]
fn store_a_text_file_beginning_with_bm_is_not_a_medium() {
    refused("text beginning with BM", b"BMW owners club, minutes of the meeting of 3 May.\n");
    refused("BM, then nothing coherent", b"BM\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xff\xff\xff\xff");
}

// A medium must be a medium (a reading, reported): the neighbours of listed formats — the same containers, another form — are not on the list, so not media.
#[test]
fn store_neighbours_of_listed_formats_are_not_media() {
    refused("WAVE audio (RIFF, form WAVE)", b"RIFF\x24\x00\x00\x00WAVEfmt \x10\x00\x00\x00");
    refused("M4A audio (ftyp M4A)", b"\x00\x00\x00\x20ftypM4A \x00\x00\x00\x00M4A mp42isom");
    refused("XML that is not SVG", b"<?xml version=\"1.0\"?>\n<note><to>me</to></note>");
    refused("HTML", b"<!DOCTYPE html>\n<html><body><script>");
}
