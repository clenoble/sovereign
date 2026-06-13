//! Shared helpers for talking to the loopback Python sidecars
//! (jiminy-bridge on :9100, jiminy-vision on :9101).

/// Build default headers for sidecar requests: when `JIMINY_TOKEN` is set,
/// every request carries `Authorization: Bearer <token>` so the (loopback)
/// sidecars can reject requests from other local processes. Both Python
/// sidecars read the same env var.
pub fn auth_headers() -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    if let Ok(token) = std::env::var("JIMINY_TOKEN") {
        if !token.is_empty() {
            if let Ok(mut value) =
                reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            {
                value.set_sensitive(true);
                headers.insert(reqwest::header::AUTHORIZATION, value);
            }
        }
    }
    headers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_token_means_no_headers() {
        // JIMINY_TOKEN is not set in the test environment by default.
        if std::env::var("JIMINY_TOKEN").is_err() {
            assert!(auth_headers().is_empty());
        }
    }
}
