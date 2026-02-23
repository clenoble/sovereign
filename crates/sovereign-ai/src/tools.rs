//! Tool definitions, parsing, and execution for the chat agent loop.
//!
//! Tools are read-only (Observe level) — they query the database but never
//! modify data. Write operations go through the intent classifier and action
//! gate system which enforces trust and confirmation per the UX principles.

use serde::Deserialize;
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

/// Definition of a tool the model can call.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: &'static str,
}

/// A parsed tool call from model output.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of executing a tool.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
}

/// Read-only tools (Observe level — no confirmation needed).
pub const READ_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "search_documents",
        description: "Search documents by title or content keyword. Returns matching titles and IDs.",
        parameters: r#"{"query": "search term"}"#,
    },
    ToolDef {
        name: "list_threads",
        description: "List all threads (projects) with document counts.",
        parameters: "{}",
    },
    ToolDef {
        name: "get_document",
        description: "Get the full content of a document by title.",
        parameters: r#"{"title": "document title"}"#,
    },
    ToolDef {
        name: "list_documents",
        description: "List documents, optionally filtered by thread name.",
        parameters: r#"{"thread": "thread name (optional)"}"#,
    },
    ToolDef {
        name: "search_messages",
        description: "Search conversation messages by keyword.",
        parameters: r#"{"query": "search term"}"#,
    },
    ToolDef {
        name: "list_contacts",
        description: "List all contacts with their communication channels.",
        parameters: "{}",
    },
];

/// Write tools (Modify level — require action-gate confirmation).
pub const WRITE_TOOLS: &[ToolDef] = &[
    ToolDef {
        name: "create_document",
        description: "Create a new owned document. Requires user confirmation.",
        parameters: r#"{"title": "document title", "thread_name": "thread name (optional, defaults to first thread)"}"#,
    },
    ToolDef {
        name: "create_thread",
        description: "Create a new thread (project). Requires user confirmation.",
        parameters: r#"{"name": "thread name"}"#,
    },
    ToolDef {
        name: "rename_thread",
        description: "Rename an existing thread. Requires user confirmation.",
        parameters: r#"{"old_name": "current thread name", "new_name": "new thread name"}"#,
    },
    ToolDef {
        name: "move_document",
        description: "Move a document to a different thread. Requires user confirmation.",
        parameters: r#"{"document_title": "document title", "thread_name": "destination thread name"}"#,
    },
];

/// All available tools (read + write).
pub const TOOLS: &[ToolDef] = &[
    // Read tools (Observe level)
    ToolDef {
        name: "search_documents",
        description: "Search documents by title or content keyword. Returns matching titles and IDs.",
        parameters: r#"{"query": "search term"}"#,
    },
    ToolDef {
        name: "list_threads",
        description: "List all threads (projects) with document counts.",
        parameters: "{}",
    },
    ToolDef {
        name: "get_document",
        description: "Get the full content of a document by title.",
        parameters: r#"{"title": "document title"}"#,
    },
    ToolDef {
        name: "list_documents",
        description: "List documents, optionally filtered by thread name.",
        parameters: r#"{"thread": "thread name (optional)"}"#,
    },
    ToolDef {
        name: "search_messages",
        description: "Search conversation messages by keyword.",
        parameters: r#"{"query": "search term"}"#,
    },
    ToolDef {
        name: "list_contacts",
        description: "List all contacts with their communication channels.",
        parameters: "{}",
    },
    // Write tools (Modify level — gated by action gravity)
    ToolDef {
        name: "create_document",
        description: "Create a new owned document. Requires user confirmation.",
        parameters: r#"{"title": "document title", "thread_name": "thread name (optional, defaults to first thread)"}"#,
    },
    ToolDef {
        name: "create_thread",
        description: "Create a new thread (project). Requires user confirmation.",
        parameters: r#"{"name": "thread name"}"#,
    },
    ToolDef {
        name: "rename_thread",
        description: "Rename an existing thread. Requires user confirmation.",
        parameters: r#"{"old_name": "current thread name", "new_name": "new thread name"}"#,
    },
    ToolDef {
        name: "move_document",
        description: "Move a document to a different thread. Requires user confirmation.",
        parameters: r#"{"document_title": "document title", "thread_name": "destination thread name"}"#,
    },
];

/// Format tool definitions as a text block for the system prompt.
pub fn format_tool_descriptions() -> String {
    let mut out = String::from("You have access to these tools:\n");
    for tool in TOOLS {
        out.push_str(&format!(
            "- {}: {} Parameters: {}\n",
            tool.name, tool.description, tool.parameters
        ));
    }
    out.push_str(
        "\nTo use a tool, output:\n\
         <tool_call>\n\
         {\"name\": \"tool_name\", \"arguments\": {\"key\": \"value\"}}\n\
         </tool_call>\n\n\
         You can call one tool per turn. After seeing the result, either call another tool or give your final answer.\n\
         If you can answer without tools, respond directly.\n",
    );
    out
}

/// Check if model output contains a tool call.
pub fn has_tool_call(output: &str) -> bool {
    output.contains("<tool_call>")
}

/// Parse tool calls from model output.
/// Looks for `<tool_call>...</tool_call>` delimiters containing JSON.
pub fn parse_tool_calls(output: &str) -> Vec<ToolCall> {
    let mut calls = Vec::new();
    let mut remaining = output;

    while let Some(start) = remaining.find("<tool_call>") {
        let after_tag = &remaining[start + 11..];
        if let Some(end) = after_tag.find("</tool_call>") {
            let json_str = after_tag[..end].trim();
            if let Ok(call) = serde_json::from_str::<ToolCall>(json_str) {
                calls.push(call);
            }
            remaining = &after_tag[end + 12..];
        } else {
            break;
        }
    }
    calls
}

/// Extract the text response (non-tool-call portion) from model output.
pub fn extract_text_response(output: &str) -> String {
    let mut text = output.to_string();
    // Remove all <tool_call>...</tool_call> blocks
    while let Some(start) = text.find("<tool_call>") {
        if let Some(end_offset) = text[start..].find("</tool_call>") {
            text.replace_range(start..start + end_offset + 12, "");
        } else {
            break;
        }
    }
    text.trim().to_string()
}

/// Check if a tool name is a write tool (requires action-gate confirmation).
pub fn is_write_tool(name: &str) -> bool {
    matches!(
        name,
        "create_document" | "create_thread" | "rename_thread" | "move_document"
    )
}

/// Execute a write tool call against the database. Returns the result.
/// The caller is responsible for gating (confirmation) before calling this.
pub async fn execute_write_tool(call: &ToolCall, db: &SurrealGraphDB) -> WriteToolResult {
    match call.name.as_str() {
        "create_document" => execute_create_document(call, db).await,
        "create_thread" => execute_create_thread(call, db).await,
        "rename_thread" => execute_rename_thread(call, db).await,
        "move_document" => execute_move_document(call, db).await,
        _ => WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: format!("Unknown write tool: {}", call.name),
            event: None,
        },
    }
}

/// Result from a write tool, including an optional OrchestratorEvent to emit.
#[derive(Debug, Clone)]
pub struct WriteToolResult {
    pub tool_name: String,
    pub success: bool,
    pub output: String,
    pub event: Option<sovereign_core::interfaces::OrchestratorEvent>,
}

async fn execute_create_document(call: &ToolCall, db: &SurrealGraphDB) -> WriteToolResult {
    let title = call
        .arguments
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Untitled Document")
        .to_string();

    let thread_name = call.arguments.get("thread_name").and_then(|v| v.as_str());

    // Resolve thread ID
    let threads = db.list_threads().await.unwrap_or_default();
    let thread = if let Some(tname) = thread_name {
        let tname_lower = tname.to_lowercase();
        threads
            .iter()
            .find(|t| t.name.to_lowercase().contains(&tname_lower))
            .or(threads.first())
    } else {
        threads.first()
    };

    let thread_id = thread
        .and_then(|t| t.id_string())
        .unwrap_or_default();

    let doc = sovereign_db::schema::Document::new(title.clone(), thread_id.clone(), true);
    match db.create_document(doc).await {
        Ok(created) => {
            let doc_id = created.id_string().unwrap_or_default();
            WriteToolResult {
                tool_name: call.name.clone(),
                success: true,
                output: format!("Created document '{}' (id: {}) in thread {}", title, doc_id, thread_id),
                event: Some(sovereign_core::interfaces::OrchestratorEvent::DocumentCreated {
                    doc_id,
                    title,
                    thread_id,
                }),
            }
        }
        Err(e) => WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: format!("Failed to create document: {e}"),
            event: None,
        },
    }
}

async fn execute_create_thread(call: &ToolCall, db: &SurrealGraphDB) -> WriteToolResult {
    let name = call
        .arguments
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("New Thread")
        .to_string();

    let thread = sovereign_db::schema::Thread::new(name.clone(), String::new());
    match db.create_thread(thread).await {
        Ok(created) => {
            let tid = created.id_string().unwrap_or_default();
            WriteToolResult {
                tool_name: call.name.clone(),
                success: true,
                output: format!("Created thread '{}' (id: {})", name, tid),
                event: Some(sovereign_core::interfaces::OrchestratorEvent::ThreadCreated {
                    thread_id: tid,
                    name,
                }),
            }
        }
        Err(e) => WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: format!("Failed to create thread: {e}"),
            event: None,
        },
    }
}

async fn execute_rename_thread(call: &ToolCall, db: &SurrealGraphDB) -> WriteToolResult {
    let old_name = call
        .arguments
        .get("old_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new_name = call
        .arguments
        .get("new_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if old_name.is_empty() || new_name.is_empty() {
        return WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: "Both old_name and new_name are required.".into(),
            event: None,
        };
    }

    let threads = db.list_threads().await.unwrap_or_default();
    let old_lower = old_name.to_lowercase();
    let thread = threads
        .iter()
        .find(|t| t.name.to_lowercase().contains(&old_lower));

    if let Some(thread) = thread {
        let tid = thread.id_string().unwrap_or_default();
        match db.update_thread(&tid, Some(new_name), None).await {
            Ok(_) => WriteToolResult {
                tool_name: call.name.clone(),
                success: true,
                output: format!("Renamed thread '{}' to '{}'", old_name, new_name),
                event: Some(sovereign_core::interfaces::OrchestratorEvent::ThreadRenamed {
                    thread_id: tid,
                    name: new_name.to_string(),
                }),
            },
            Err(e) => WriteToolResult {
                tool_name: call.name.clone(),
                success: false,
                output: format!("Failed to rename thread: {e}"),
                event: None,
            },
        }
    } else {
        WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: format!("Thread '{}' not found.", old_name),
            event: None,
        }
    }
}

async fn execute_move_document(call: &ToolCall, db: &SurrealGraphDB) -> WriteToolResult {
    let doc_title = call
        .arguments
        .get("document_title")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let thread_name = call
        .arguments
        .get("thread_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    if doc_title.is_empty() || thread_name.is_empty() {
        return WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: "Both document_title and thread_name are required.".into(),
            event: None,
        };
    }

    let docs = db.list_documents(None).await.unwrap_or_default();
    let threads = db.list_threads().await.unwrap_or_default();

    let doc_lower = doc_title.to_lowercase();
    let thread_lower = thread_name.to_lowercase();
    let doc = docs
        .iter()
        .find(|d| d.title.to_lowercase().contains(&doc_lower));
    let thread = threads
        .iter()
        .find(|t| t.name.to_lowercase().contains(&thread_lower));

    if let (Some(doc), Some(thread)) = (doc, thread) {
        let doc_id = doc.id_string().unwrap_or_default();
        let tid = thread.id_string().unwrap_or_default();
        match db.move_document_to_thread(&doc_id, &tid).await {
            Ok(_) => WriteToolResult {
                tool_name: call.name.clone(),
                success: true,
                output: format!("Moved '{}' to thread '{}'", doc_title, thread_name),
                event: Some(sovereign_core::interfaces::OrchestratorEvent::DocumentMoved {
                    doc_id,
                    new_thread_id: tid,
                }),
            },
            Err(e) => WriteToolResult {
                tool_name: call.name.clone(),
                success: false,
                output: format!("Failed to move document: {e}"),
                event: None,
            },
        }
    } else {
        let mut msg = String::new();
        if doc.is_none() {
            msg.push_str(&format!("Document '{}' not found. ", doc_title));
        }
        if thread.is_none() {
            msg.push_str(&format!("Thread '{}' not found.", thread_name));
        }
        WriteToolResult {
            tool_name: call.name.clone(),
            success: false,
            output: msg,
            event: None,
        }
    }
}

/// Execute a read-only tool call against the database. Returns a result with truncated output.
pub async fn execute_tool(call: &ToolCall, db: &SurrealGraphDB) -> ToolResult {
    let output = match call.name.as_str() {
        "search_documents" => execute_search_documents(call, db).await,
        "list_threads" => execute_list_threads(db).await,
        "get_document" => execute_get_document(call, db).await,
        "list_documents" => execute_list_documents(call, db).await,
        "search_messages" => execute_search_messages(call, db).await,
        "list_contacts" => execute_list_contacts(db).await,
        _ => format!("Unknown tool: {}", call.name),
    };

    ToolResult {
        tool_name: call.name.clone(),
        success: !output.starts_with("Unknown tool"),
        output,
    }
}

async fn execute_search_documents(call: &ToolCall, db: &SurrealGraphDB) -> String {
    let query = call
        .arguments
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let query_lower = query.to_lowercase();

    let docs = db.list_documents(None).await.unwrap_or_default();
    let matches: Vec<String> = docs
        .iter()
        .filter(|d| d.title.to_lowercase().contains(&query_lower))
        .take(8)
        .map(|d| {
            let ownership = if d.is_owned { "owned" } else { "external" };
            format!("- {} ({})", d.title, ownership)
        })
        .collect();

    if matches.is_empty() {
        format!("No documents found matching '{query}'.")
    } else {
        format!("Found {} documents:\n{}", matches.len(), matches.join("\n"))
    }
}

async fn execute_list_threads(db: &SurrealGraphDB) -> String {
    let threads = db.list_threads().await.unwrap_or_default();
    let docs = db.list_documents(None).await.unwrap_or_default();

    let lines: Vec<String> = threads
        .iter()
        .map(|t| {
            let tid = t.id_string().unwrap_or_default();
            let count = docs.iter().filter(|d| d.thread_id == tid).count();
            format!("- {} ({} documents)", t.name, count)
        })
        .collect();

    if lines.is_empty() {
        "No threads found.".into()
    } else {
        lines.join("\n")
    }
}

async fn execute_get_document(call: &ToolCall, db: &SurrealGraphDB) -> String {
    let title = call
        .arguments
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let title_lower = title.to_lowercase();

    let docs = db.list_documents(None).await.unwrap_or_default();
    if let Some(doc) = docs
        .iter()
        .find(|d| d.title.to_lowercase().contains(&title_lower))
    {
        let ownership = if doc.is_owned { "owned" } else { "external" };
        let body = &doc.content;
        let truncated = if body.len() > 500 { &body[..500] } else { body };
        format!(
            "Title: {} ({})\nContent:\n{}",
            doc.title, ownership, truncated
        )
    } else {
        format!("Document '{title}' not found.")
    }
}

async fn execute_list_documents(call: &ToolCall, db: &SurrealGraphDB) -> String {
    let thread_name = call.arguments.get("thread").and_then(|v| v.as_str());
    let docs = db.list_documents(None).await.unwrap_or_default();

    let filtered: Vec<_> = if let Some(tname) = thread_name {
        let tname_lower = tname.to_lowercase();
        let threads = db.list_threads().await.unwrap_or_default();
        if let Some(thread) = threads
            .iter()
            .find(|t| t.name.to_lowercase().contains(&tname_lower))
        {
            let tid = thread.id_string().unwrap_or_default();
            docs.iter().filter(|d| d.thread_id == tid).collect()
        } else {
            return format!("Thread '{tname}' not found.");
        }
    } else {
        docs.iter().collect()
    };

    let lines: Vec<String> = filtered
        .iter()
        .take(15)
        .map(|d| {
            let ownership = if d.is_owned { "owned" } else { "external" };
            format!("- {} ({})", d.title, ownership)
        })
        .collect();

    if lines.is_empty() {
        "No documents found.".into()
    } else {
        format!("{} documents:\n{}", filtered.len(), lines.join("\n"))
    }
}

async fn execute_search_messages(call: &ToolCall, db: &SurrealGraphDB) -> String {
    let query = call
        .arguments
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    match db.search_messages(query).await {
        Ok(msgs) => {
            let lines: Vec<String> = msgs
                .iter()
                .take(5)
                .map(|m| {
                    let body_preview = if m.body.len() > 100 {
                        &m.body[..100]
                    } else {
                        &m.body
                    };
                    format!("- [{}] {}", m.sent_at.format("%Y-%m-%d"), body_preview)
                })
                .collect();

            if lines.is_empty() {
                format!("No messages matching '{query}'.")
            } else {
                format!(
                    "Found {} messages:\n{}",
                    msgs.len().min(5),
                    lines.join("\n")
                )
            }
        }
        Err(e) => format!("Message search failed: {e}"),
    }
}

async fn execute_list_contacts(db: &SurrealGraphDB) -> String {
    let contacts = db.list_contacts().await.unwrap_or_default();

    let lines: Vec<String> = contacts
        .iter()
        .take(10)
        .map(|c| {
            let channels: Vec<String> = c.addresses.iter().map(|a| format!("{}", a.channel)).collect();
            let ownership = if c.is_owned { "you" } else { "contact" };
            format!("- {} ({}, {})", c.name, ownership, channels.join(", "))
        })
        .collect();

    if lines.is_empty() {
        "No contacts found.".into()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tool_calls_valid() {
        let output = r#"Let me search for that.
<tool_call>
{"name": "search_documents", "arguments": {"query": "meeting notes"}}
</tool_call>"#;
        let calls = parse_tool_calls(output);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "search_documents");
        assert_eq!(
            calls[0].arguments.get("query").unwrap().as_str().unwrap(),
            "meeting notes"
        );
    }

    #[test]
    fn parse_tool_calls_empty() {
        let output = "Hello! I can help with that.";
        let calls = parse_tool_calls(output);
        assert!(calls.is_empty());
    }

    #[test]
    fn parse_tool_calls_malformed_json() {
        let output = "<tool_call>\n{not valid json}\n</tool_call>";
        let calls = parse_tool_calls(output);
        assert!(calls.is_empty());
    }

    #[test]
    fn parse_tool_calls_no_closing_tag() {
        let output = "<tool_call>\n{\"name\": \"test\", \"arguments\": {}}";
        let calls = parse_tool_calls(output);
        assert!(calls.is_empty());
    }

    #[test]
    fn has_tool_call_true() {
        assert!(has_tool_call("some text <tool_call> stuff </tool_call>"));
    }

    #[test]
    fn has_tool_call_false() {
        assert!(!has_tool_call("just a normal response"));
    }

    #[test]
    fn extract_text_response_strips_tool_blocks() {
        let output = "Here's what I found:\n<tool_call>\n{\"name\":\"search\",\"arguments\":{}}\n</tool_call>\nDone!";
        let text = extract_text_response(output);
        assert_eq!(text, "Here's what I found:\n\nDone!");
    }

    #[test]
    fn extract_text_response_no_tool() {
        let output = "Just a response.";
        let text = extract_text_response(output);
        assert_eq!(text, "Just a response.");
    }

    #[test]
    fn format_tool_descriptions_contains_all_tools() {
        let desc = format_tool_descriptions();
        for tool in TOOLS {
            assert!(desc.contains(tool.name), "Missing tool: {}", tool.name);
        }
        assert!(desc.contains("<tool_call>"));
    }

    #[test]
    fn is_write_tool_checks() {
        assert!(is_write_tool("create_document"));
        assert!(is_write_tool("create_thread"));
        assert!(is_write_tool("rename_thread"));
        assert!(is_write_tool("move_document"));
        assert!(!is_write_tool("search_documents"));
        assert!(!is_write_tool("list_threads"));
        assert!(!is_write_tool("get_document"));
    }

    #[test]
    fn write_tools_have_correct_names() {
        let names: Vec<&str> = WRITE_TOOLS.iter().map(|t| t.name).collect();
        assert!(names.contains(&"create_document"));
        assert!(names.contains(&"create_thread"));
        assert!(names.contains(&"rename_thread"));
        assert!(names.contains(&"move_document"));
    }

    #[test]
    fn read_tools_have_correct_names() {
        let names: Vec<&str> = READ_TOOLS.iter().map(|t| t.name).collect();
        assert!(names.contains(&"search_documents"));
        assert!(names.contains(&"list_threads"));
        assert!(!names.contains(&"create_document"));
    }
}
