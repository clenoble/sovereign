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

/// All available tools (read-only, Observe level).
pub const TOOLS: &[ToolDef] = &[
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

/// Execute a tool call against the database. Returns a result with truncated output.
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
}
