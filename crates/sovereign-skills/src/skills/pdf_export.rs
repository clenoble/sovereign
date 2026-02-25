use std::io::Cursor;

use crate::traits::{CoreSkill, SkillDocument, SkillOutput};

/// Try multiple font paths for cross-platform support.
fn load_font_family() -> anyhow::Result<genpdf::fonts::FontFamily<genpdf::fonts::FontData>> {
    let candidates = [
        // Linux (Debian/Ubuntu)
        ("/usr/share/fonts/truetype/liberation", "LiberationSans"),
        // Windows
        ("C:/Windows/Fonts", "arial"),
        // macOS
        ("/System/Library/Fonts/Supplemental", "Arial"),
    ];
    for (path, name) in &candidates {
        if let Ok(family) = genpdf::fonts::from_files(path, name, None) {
            return Ok(family);
        }
    }
    anyhow::bail!("No suitable font found. Checked: {}", candidates.iter().map(|(p, _)| *p).collect::<Vec<_>>().join(", "))
}

pub struct PdfExportSkill;

impl CoreSkill for PdfExportSkill {
    fn name(&self) -> &str {
        "pdf-export"
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
        _params: &str,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "export" => {
                let font_family = load_font_family()?;

                let mut pdf = genpdf::Document::new(font_family);
                pdf.set_title(&doc.title);

                let decorator = genpdf::SimplePageDecorator::new();
                pdf.set_page_decorator(decorator);

                // Title
                let mut title_style = genpdf::style::Style::new();
                title_style.set_font_size(18);
                title_style.set_bold();
                pdf.push(genpdf::elements::Paragraph::new(
                    genpdf::style::StyledString::new(doc.title.clone(), title_style),
                ));
                pdf.push(genpdf::elements::Break::new(1.0));

                // Body text — split by lines
                for line in doc.content.body.lines() {
                    if line.is_empty() {
                        pdf.push(genpdf::elements::Break::new(0.5));
                    } else {
                        pdf.push(genpdf::elements::Paragraph::new(line));
                    }
                }

                // Image list (as text references, not embedded)
                if !doc.content.images.is_empty() {
                    pdf.push(genpdf::elements::Break::new(1.0));
                    let mut img_header = genpdf::style::Style::new();
                    img_header.set_font_size(14);
                    img_header.set_bold();
                    pdf.push(genpdf::elements::Paragraph::new(
                        genpdf::style::StyledString::new("Images".to_string(), img_header),
                    ));
                    for img in &doc.content.images {
                        let label = if img.caption.is_empty() {
                            img.path.clone()
                        } else {
                            format!("{} — {}", img.path, img.caption)
                        };
                        pdf.push(genpdf::elements::Paragraph::new(format!("  - {label}")));
                    }
                }

                let mut buf = Vec::new();
                pdf.render(&mut Cursor::new(&mut buf))
                    .map_err(|e| anyhow::anyhow!("PDF render failed: {e}"))?;

                Ok(SkillOutput::File {
                    name: format!("{}.pdf", doc.title),
                    mime_type: "application/pdf".into(),
                    data: buf,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("export".into(), "Export PDF".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::content::{ContentFields, ContentImage};

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:test".into(),
            title: "Test Document".into(),
            content: ContentFields {
                body: "# Hello\n\nThis is a test document.\n\nWith multiple paragraphs.".into(),
                images: vec![ContentImage {
                    path: "/tmp/test.png".into(),
                    caption: "A test image".into(),
                }],
                ..Default::default()
            },
        }
    }

    #[test]
    fn export_returns_nonempty_pdf_bytes() {
        let skill = PdfExportSkill;
        let doc = make_doc();
        let result = skill.execute("export", &doc, "");
        // May fail if fonts not installed — that's expected in CI
        match result {
            Ok(SkillOutput::File { name, mime_type, data }) => {
                assert!(name.contains("Test Document"));
                assert_eq!(mime_type, "application/pdf");
                assert!(!data.is_empty());
                // Check PDF magic bytes
                assert_eq!(&data[..5], b"%PDF-");
            }
            Err(e) => {
                // Font not found is OK in test environments
                let msg = e.to_string();
                assert!(
                    msg.contains("font") || msg.contains("Font"),
                    "Unexpected error: {msg}"
                );
            }
            _ => panic!("Expected File output"),
        }
    }

    #[test]
    fn actions_returns_export() {
        let skill = PdfExportSkill;
        let actions = skill.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].0, "export");
    }
}
