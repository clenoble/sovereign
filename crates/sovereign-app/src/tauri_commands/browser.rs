use super::*;

// ---------------------------------------------------------------------------
// Phase 5: Comms configuration
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CommsConfigDto {
    pub comms_available: bool,
    pub email_configured: bool,
    pub email_imap_host: String,
    pub email_imap_port: u16,
    pub email_smtp_host: String,
    pub email_smtp_port: u16,
    pub email_username: String,
    pub signal_configured: bool,
    pub signal_phone: String,
}

/// Return the current comms configuration.
#[tauri::command]
pub async fn get_comms_config(state: State<'_, AppState>) -> Result<CommsConfigDto, String> {
    state.require_unlocked().await?;
    #[cfg(feature = "comms")]
    {
        // Load comms config from disk
        let config_path = sovereign_core::sovereign_dir().join("comms.toml");
        if config_path.exists() {
            let data = std::fs::read_to_string(&config_path).str_err()?;
            let cfg: sovereign_comms::config::CommsConfig =
                toml::from_str(&data).str_err()?;
            let (email_configured, imap_host, imap_port, smtp_host, smtp_port, username) =
                if let Some(ref email) = cfg.email {
                    (
                        true,
                        email.imap_host.clone(),
                        email.imap_port,
                        email.smtp_host.clone(),
                        email.smtp_port,
                        email.username.clone(),
                    )
                } else {
                    (false, String::new(), 993, String::new(), 587, String::new())
                };
            let (signal_configured, signal_phone) = if let Some(ref signal) = cfg.signal {
                (true, signal.phone_number.clone())
            } else {
                (false, String::new())
            };
            return Ok(CommsConfigDto {
                comms_available: true,
                email_configured,
                email_imap_host: imap_host,
                email_imap_port: imap_port,
                email_smtp_host: smtp_host,
                email_smtp_port: smtp_port,
                email_username: username,
                signal_configured,
                signal_phone,
            });
        }
        return Ok(CommsConfigDto {
            comms_available: true,
            email_configured: false,
            email_imap_host: String::new(),
            email_imap_port: 993,
            email_smtp_host: String::new(),
            email_smtp_port: 587,
            email_username: String::new(),
            signal_configured: false,
            signal_phone: String::new(),
        });
    }
    #[cfg(not(feature = "comms"))]
    Ok(CommsConfigDto {
        comms_available: false,
        email_configured: false,
        email_imap_host: String::new(),
        email_imap_port: 993,
        email_smtp_host: String::new(),
        email_smtp_port: 587,
        email_username: String::new(),
        signal_configured: false,
        signal_phone: String::new(),
    })
}

#[derive(Deserialize)]
pub struct SaveCommsConfigDto {
    pub email_imap_host: Option<String>,
    pub email_imap_port: Option<u16>,
    pub email_smtp_host: Option<String>,
    pub email_smtp_port: Option<u16>,
    pub email_username: Option<String>,
    pub signal_phone: Option<String>,
}

/// Save comms configuration to disk.
#[tauri::command]
pub async fn save_comms_config(
    state: State<'_, AppState>,
    data: SaveCommsConfigDto,
) -> Result<(), String> {
    state.require_unlocked().await?;
    #[cfg(feature = "comms")]
    {
        let config_dir = sovereign_core::sovereign_dir();
        std::fs::create_dir_all(&config_dir).str_err()?;

        // IPC-002: build a typed CommsConfig and serialize it with the toml
        // crate instead of hand-formatting strings (which let unescaped quotes
        // / newlines forge arbitrary config keys). Validate hosts + ports first.
        let mut cfg = sovereign_comms::config::CommsConfig {
            enabled: true,
            ..Default::default()
        };

        if let Some(ref host) = data.email_imap_host {
            if !host.is_empty() {
                validate_host(host)?;
                let imap_port = data.email_imap_port.unwrap_or(993);
                validate_port(imap_port)?;
                let smtp_host = data.email_smtp_host.clone().unwrap_or_default();
                if !smtp_host.is_empty() {
                    validate_host(&smtp_host)?;
                }
                let smtp_port = data.email_smtp_port.unwrap_or(587);
                validate_port(smtp_port)?;
                cfg.email = Some(sovereign_comms::config::EmailAccountConfig {
                    imap_host: host.clone(),
                    imap_port,
                    smtp_host,
                    smtp_port,
                    username: data.email_username.clone().unwrap_or_default(),
                    display_name: None,
                });
            }
        }

        if let Some(ref phone) = data.signal_phone {
            if !phone.is_empty() {
                validate_host(phone)?; // reject quotes/newlines/control chars
                cfg.signal = Some(sovereign_comms::config::SignalAccountConfig {
                    phone_number: phone.clone(),
                    store_path: sovereign_core::sovereign_dir()
                        .join("signal")
                        .to_string_lossy()
                        .into_owned(),
                    device_name: None,
                });
            }
        }

        let serialized = toml::to_string(&cfg).str_err()?;
        let config_path = config_dir.join("comms.toml");
        std::fs::write(&config_path, serialized).str_err()?;
        return Ok(());
    }
    #[cfg(not(feature = "comms"))]
    {
        let _ = data;
        Err("Comms feature not enabled".into())
    }
}

/// Reject host-like strings containing quotes, newlines, or other control
/// characters — these would let a forged value break out of the TOML
/// string and inject arbitrary keys (IPC-002).
#[cfg(feature = "comms")]
fn validate_host(value: &str) -> Result<(), String> {
    if value
        .chars()
        .any(|c| c == '"' || c == '\'' || c == '\n' || c == '\r' || c.is_control())
    {
        return Err("Invalid host/identifier: contains quotes, newlines, or control characters".to_string());
    }
    Ok(())
}

/// Reject out-of-range ports. u16 already bounds the upper end; this
/// rejects port 0 to satisfy the documented 1..=65535 range (IPC-002).
#[cfg(feature = "comms")]
fn validate_port(port: u16) -> Result<(), String> {
    if !(1..=65535).contains(&port) {
        return Err(format!("Invalid port: {port} (must be 1..=65535)"));
    }
    Ok(())
}


// ---------------------------------------------------------------------------
// Embedded Browser (desktop only — see crate::browser for the cfg rationale)
// ---------------------------------------------------------------------------

/// Open the embedded browser webview (or navigate if already open).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn open_browser(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    url: String,
    bounds: crate::browser::LogicalRect,
) -> Result<(), String> {
    state.require_unlocked().await?;
    // Must spawn on a separate thread to avoid deadlock on Windows
    let handle = app.clone();
    tokio::task::spawn_blocking(move || {
        crate::browser::create_browser_webview(&handle, &url, bounds)
    })
    .await
    .str_err()?
}

/// Close the embedded browser webview.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn close_browser(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::destroy_browser(&app)
}

/// Navigate the browser to a new URL.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn navigate_browser(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    url: String,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::navigate_browser(&app, &url)
}

/// Go back in browser history.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn browser_back(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::browser_back(&app)
}

/// Go forward in browser history.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn browser_forward(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::browser_forward(&app)
}

/// Reload the browser page.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn browser_refresh(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::browser_refresh(&app)
}

/// Update browser webview position and size.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn set_browser_bounds(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    bounds: crate::browser::LogicalRect,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::set_browser_bounds(&app, bounds)
}

/// Show or hide the browser webview.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
#[tauri::command]
pub async fn set_browser_visible(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    visible: bool,
) -> Result<(), String> {
    state.require_unlocked().await?;
    crate::browser::set_browser_visible(&app, visible)
}

// ---------------------------------------------------------------------------
// Web Browsing — Fetch & Reliability
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct FetchedPageDto {
    pub url: String,
    pub title: String,
    pub body_markdown: String,
    pub raw_text: String,
}

#[derive(Serialize)]
pub struct ReliabilityResultDto {
    pub classification: String,
    pub final_score: f32,
    pub raw_assessment: Vec<RubricScoreDto>,
}

#[derive(Serialize)]
pub struct RubricScoreDto {
    pub indicator: String,
    pub analysis: String,
    pub score: f32,
}

/// Fetch a web page and extract readable content (server-side).
#[tauri::command]
pub async fn fetch_web_page(
    state: State<'_, AppState>,
    url: String,
) -> Result<FetchedPageDto, String> {
    state.require_unlocked().await?;
    #[cfg(feature = "web-browse")]
    {
        let page = crate::web::fetch_and_extract(&url)
            .await
            .str_err()?;
        return Ok(FetchedPageDto {
            url: page.url,
            title: page.title,
            body_markdown: page.content_html,
            raw_text: page.text,
        });
    }
    #[cfg(not(feature = "web-browse"))]
    {
        let _ = url;
        Err("Web browsing feature not enabled".into())
    }
}

/// Run reliability assessment on text content using local LLM.
#[tauri::command]
pub async fn assess_reliability(
    state: State<'_, AppState>,
    text: String,
) -> Result<ReliabilityResultDto, String> {
    state.require_unlocked().await?;
    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    let result = orch
        .assess_reliability(&text)
        .await
        .str_err()?;
    Ok(ReliabilityResultDto {
        classification: result.classification,
        final_score: result.final_score,
        raw_assessment: result
            .raw_assessment
            .into_iter()
            .map(|s| RubricScoreDto {
                indicator: s.indicator,
                analysis: s.analysis,
                score: s.score,
            })
            .collect(),
    })
}

/// Save a fetched web page as an external document with reliability metadata.
#[tauri::command]
pub async fn save_web_page(
    state: State<'_, AppState>,
    url: String,
    title: String,
    content: String,
    thread_id: Option<String>,
    classification: Option<String>,
    score: Option<f32>,
    assessment_json: Option<String>,
) -> Result<CanvasDocDto, String> {
    state.require_unlocked().await?;
    let tid = match thread_id {
        Some(t) if !t.is_empty() => t,
        _ => {
            // Create or find a "Web" thread for browsed content
            match state.db.find_thread_by_name("Web").await {
                Ok(Some(t)) => t.id_string().unwrap_or_default(),
                _ => {
                    let thread =
                        sovereign_db::schema::Thread::new("Web".into(), "Browsed web content".into());
                    let created = state
                        .db
                        .create_thread(thread)
                        .await
                        .str_err()?;
                    created.id_string().unwrap_or_default()
                }
            }
        }
    };

    let content_json = serde_json::json!({
        "body": content,
        "images": [],
        "videos": []
    })
    .to_string();

    let mut doc = Document::new(title, tid.clone(), false);
    doc.content = content_json;
    doc.source_url = Some(url);
    doc.reliability_classification = classification.clone();
    doc.reliability_score = score;
    doc.reliability_assessment = assessment_json;
    if classification.is_some() || score.is_some() {
        doc.assessed_at = Some(Utc::now());
    }

    let created = state
        .db
        .create_document(doc)
        .await
        .str_err()?;

    let id = created.id_string().unwrap_or_default();

    Ok(CanvasDocDto {
        id,
        title: created.title,
        thread_id: tid,
        is_owned: false,
        spatial_x: created.spatial_x,
        spatial_y: created.spatial_y,
        created_at: created.created_at.to_rfc3339(),
        modified_at: created.modified_at.to_rfc3339(),
        reliability_classification: created.reliability_classification,
        reliability_score: created.reliability_score,
        source_url: created.source_url,
    })
}

/// Re-run reliability assessment on an existing document.
#[tauri::command]
pub async fn reassess_reliability(
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<ReliabilityResultDto, String> {
    state.require_unlocked().await?;
    let doc = state.db.get_document(&doc_id).await.str_err()?;

    // Parse content JSON to extract body text
    let body = if let Ok(v) = serde_json::from_str::<serde_json::Value>(&doc.content) {
        v["body"].as_str().unwrap_or("").to_string()
    } else {
        doc.content.clone()
    };

    if body.is_empty() {
        return Err("Document has no text content to assess".into());
    }

    let orch = state.orchestrator.as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    let result = orch
        .assess_reliability(&body)
        .await
        .str_err()?;

    let assessment_json = sovereign_ai::reliability::assessment_to_json(&result);

    // Update document with new reliability data
    state
        .db
        .update_document_reliability(
            &doc_id,
            None,
            Some(&result.classification),
            Some(result.final_score),
            Some(&assessment_json),
        )
        .await
        .str_err()?;

    Ok(ReliabilityResultDto {
        classification: result.classification,
        final_score: result.final_score,
        raw_assessment: result
            .raw_assessment
            .into_iter()
            .map(|s| RubricScoreDto {
                indicator: s.indicator,
                analysis: s.analysis,
                score: s.score,
            })
            .collect(),
    })
}

