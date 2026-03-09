//! Web content fetching and extraction for the Sovereign Browser.
//!
//! Uses `reqwest` for HTTP fetching and `readability` for article extraction.

use std::io::Cursor;
use std::time::Duration;

use url::Url;

/// A fetched and extracted web page.
#[derive(Debug, Clone)]
pub struct FetchedPage {
    pub url: String,
    pub title: String,
    /// Cleaned article HTML (from readability).
    pub content_html: String,
    /// Plain text of the article body (for LLM assessment).
    pub text: String,
}

/// Fetch a web page and extract readable content.
///
/// Uses the `readability` crate (port of arc90's readability algorithm)
/// to extract the main article content, stripping navigation, ads, etc.
pub async fn fetch_and_extract(url_str: &str) -> anyhow::Result<FetchedPage> {
    let parsed_url =
        Url::parse(url_str).map_err(|e| anyhow::anyhow!("Invalid URL '{}': {}", url_str, e))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent("Sovereign-GE/0.1 (https://github.com/clenoble/sovereign)")
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()?;

    let response = client.get(parsed_url.clone()).send().await?;

    if !response.status().is_success() {
        anyhow::bail!(
            "HTTP {} for {}",
            response.status(),
            url_str
        );
    }

    let html = response.text().await?;

    // readability::extractor::extract takes &mut Read + &Url
    let mut cursor = Cursor::new(html.as_bytes());
    let product = readability::extractor::extract(&mut cursor, &parsed_url)
        .map_err(|e| anyhow::anyhow!("Content extraction failed: {}", e))?;

    Ok(FetchedPage {
        url: url_str.to_string(),
        title: product.title,
        content_html: product.content,
        text: product.text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetched_page_struct() {
        let page = FetchedPage {
            url: "https://example.com".into(),
            title: "Example".into(),
            content_html: "<p>Hello</p>".into(),
            text: "Hello".into(),
        };
        assert_eq!(page.title, "Example");
        assert!(!page.text.is_empty());
    }
}
