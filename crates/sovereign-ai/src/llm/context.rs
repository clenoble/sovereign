//! Context gathering and multi-turn prompt assembly for the orchestrator.
//!
//! Builds workspace-aware prompts by querying the DB for current state,
//! converting session log entries into chat turns, and assembling multi-turn
//! ChatML prompts within a token budget.

use sovereign_db::GraphDB;

use crate::session_log::SessionEntry;

/// A single turn in conversation history.
#[derive(Debug, Clone)]
pub struct ChatTurn {
    pub role: ChatRole,
    pub content: String,
}

/// Role for a chat turn in the ChatML format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
    Tool,
}

/// Snapshot of the workspace state for prompt injection.
#[derive(Debug, Clone)]
pub struct WorkspaceContext {
    pub thread_count: usize,
    pub doc_count: usize,
    pub thread_names: Vec<String>,
    pub recent_doc_titles: Vec<String>,
    pub contact_count: usize,
    pub unread_conversations: usize,
}

/// Gather workspace context from the database (fast read-only queries).
pub async fn gather_workspace_context(db: &dyn GraphDB) -> WorkspaceContext {
    let threads = db.list_threads().await.unwrap_or_default();
    let docs = db.list_documents(None).await.unwrap_or_default();
    let contacts = db.list_contacts().await.unwrap_or_default();
    let conversations = db.list_conversations(None).await.unwrap_or_default();

    let unread = conversations.iter().filter(|c| c.unread_count > 0).count();

    // Sort by index to avoid cloning all titles — only clone the top 10.
    let mut indices: Vec<usize> = (0..docs.len()).collect();
    indices.sort_by(|&a, &b| docs[b].modified_at.cmp(&docs[a].modified_at));

    WorkspaceContext {
        thread_count: threads.len(),
        doc_count: docs.len(),
        thread_names: threads.iter().take(10).map(|t| t.name.clone()).collect(),
        recent_doc_titles: indices
            .iter()
            .take(10)
            .map(|&i| docs[i].title.clone())
            .collect(),
        contact_count: contacts.len(),
        unread_conversations: unread,
    }
}

/// Format workspace context as a concise text block for the system prompt.
pub fn format_workspace_context(ctx: &WorkspaceContext) -> String {
    let mut out = format!(
        "Workspace: {} threads, {} documents, {} contacts",
        ctx.thread_count, ctx.doc_count, ctx.contact_count
    );
    if ctx.unread_conversations > 0 {
        out.push_str(&format!(", {} unread conversations", ctx.unread_conversations));
    }
    out.push('\n');
    if !ctx.thread_names.is_empty() {
        out.push_str(&format!("Threads: {}\n", ctx.thread_names.join(", ")));
    }
    if !ctx.recent_doc_titles.is_empty() {
        out.push_str(&format!(
            "Recent documents: {}\n",
            ctx.recent_doc_titles.join(", ")
        ));
    }
    out
}

/// Convert session log entries into chat turns for prompt injection.
///
/// Only `user_input` entries with mode "chat" and `chat_response` entries
/// become turns. Other entries (search, action) are skipped to keep the
/// conversation focused on the chat flow.
pub fn session_entries_to_chat_turns(entries: &[SessionEntry]) -> Vec<ChatTurn> {
    let mut turns = Vec::new();
    for entry in entries {
        match entry.entry_type.as_str() {
            "user_input" => {
                if entry.mode.as_deref() == Some("chat") {
                    if let Some(ref content) = entry.content {
                        turns.push(ChatTurn {
                            role: ChatRole::User,
                            content: content.clone(),
                        });
                    }
                }
            }
            "chat_response" => {
                if let Some(ref content) = entry.content {
                    turns.push(ChatTurn {
                        role: ChatRole::Assistant,
                        content: content.clone(),
                    });
                }
            }
            _ => {}
        }
    }
    turns
}

/// Build a ChatML prompt from a full history where the conversation turns
/// (including the latest user message) are already in the history vec.
///
/// Truncates history from the oldest turns to fit within `max_history_chars`.
/// The resulting prompt ends with `<|im_start|>assistant\n` for generation.
pub fn build_prompt_from_full_history(
    system: &str,
    history: &[ChatTurn],
    max_history_chars: usize,
) -> String {
    let mut prompt = format!("<|im_start|>system\n{system}\n<|im_end|>\n");

    // Walk backward, accumulating character count to fit budget.
    let mut kept_turns: Vec<&ChatTurn> = Vec::new();
    let mut char_count = 0;
    for turn in history.iter().rev() {
        let turn_len = turn.content.len() + 30; // overhead for role tags
        if char_count + turn_len > max_history_chars {
            break;
        }
        char_count += turn_len;
        kept_turns.push(turn);
    }
    kept_turns.reverse();

    for turn in kept_turns {
        let role = match turn.role {
            ChatRole::User => "user",
            ChatRole::Assistant => "assistant",
            ChatRole::Tool => "tool",
        };
        prompt.push_str(&format!(
            "<|im_start|>{role}\n{}\n<|im_end|>\n",
            turn.content
        ));
    }

    prompt.push_str("<|im_start|>assistant\n");
    prompt
}

/// Rough character-to-token estimate for Qwen2.5 (conservative: 3.5 chars/token).
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() as f64 / 3.5).ceil() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_history_produces_valid_chatml() {
        let prompt = build_prompt_from_full_history("You are helpful.", &[], 6000);
        assert!(prompt.starts_with("<|im_start|>system\n"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn multi_turn_history_included() {
        let turns = vec![
            ChatTurn {
                role: ChatRole::User,
                content: "hello".into(),
            },
            ChatTurn {
                role: ChatRole::Assistant,
                content: "hi there".into(),
            },
            ChatTurn {
                role: ChatRole::User,
                content: "list threads".into(),
            },
        ];
        let prompt = build_prompt_from_full_history("sys", &turns, 6000);
        assert!(prompt.contains("<|im_start|>user\nhello\n<|im_end|>"));
        assert!(prompt.contains("<|im_start|>assistant\nhi there\n<|im_end|>"));
        assert!(prompt.contains("<|im_start|>user\nlist threads\n<|im_end|>"));
    }

    #[test]
    fn history_truncation_keeps_recent() {
        let turns: Vec<ChatTurn> = (0..20)
            .map(|i| ChatTurn {
                role: ChatRole::User,
                content: format!("message number {i} with some extra text for length"),
            })
            .collect();
        // Very small budget — only a few turns should fit
        let prompt = build_prompt_from_full_history("sys", &turns, 200);
        // Should NOT contain the earliest messages
        assert!(!prompt.contains("message number 0"));
        // Should contain the latest
        assert!(prompt.contains("message number 19"));
    }

    #[test]
    fn session_entries_to_chat_turns_filters_correctly() {
        let entries = vec![
            SessionEntry {
                ts: "2026-02-18T09:15:00Z".into(),
                entry_type: "user_input".into(),
                content: Some("search notes".into()),
                mode: Some("text".into()),
                intent: Some("search".into()),
                action: None,
                details: None,
            },
            SessionEntry {
                ts: "2026-02-18T14:30:00Z".into(),
                entry_type: "user_input".into(),
                content: Some("what threads?".into()),
                mode: Some("chat".into()),
                intent: Some("chat".into()),
                action: None,
                details: None,
            },
            SessionEntry {
                ts: "2026-02-18T14:30:03Z".into(),
                entry_type: "chat_response".into(),
                content: Some("You have 4 threads.".into()),
                action: None,
                details: None,
                mode: None,
                intent: None,
            },
            SessionEntry {
                ts: "2026-02-18T15:00:00Z".into(),
                entry_type: "orchestrator_action".into(),
                content: None,
                action: Some("search".into()),
                details: Some("found 1".into()),
                mode: None,
                intent: None,
            },
        ];

        let turns = session_entries_to_chat_turns(&entries);
        // Only chat user_input + chat_response should be kept
        assert_eq!(turns.len(), 2);
        assert_eq!(turns[0].role, ChatRole::User);
        assert_eq!(turns[0].content, "what threads?");
        assert_eq!(turns[1].role, ChatRole::Assistant);
        assert_eq!(turns[1].content, "You have 4 threads.");
    }

    #[test]
    fn format_workspace_context_basic() {
        let ctx = WorkspaceContext {
            thread_count: 4,
            doc_count: 14,
            thread_names: vec!["Research".into(), "Development".into()],
            recent_doc_titles: vec!["Project Plan".into(), "Budget".into()],
            contact_count: 5,
            unread_conversations: 2,
        };
        let text = format_workspace_context(&ctx);
        assert!(text.contains("4 threads"));
        assert!(text.contains("14 documents"));
        assert!(text.contains("5 contacts"));
        assert!(text.contains("2 unread"));
        assert!(text.contains("Research, Development"));
        assert!(text.contains("Project Plan, Budget"));
    }

    #[test]
    fn tool_turn_rendered_correctly() {
        let turns = vec![
            ChatTurn {
                role: ChatRole::User,
                content: "search for X".into(),
            },
            ChatTurn {
                role: ChatRole::Assistant,
                content: "<tool_call>\n{\"name\":\"search\"}\n</tool_call>".into(),
            },
            ChatTurn {
                role: ChatRole::Tool,
                content: "[search] Found 2 results.".into(),
            },
        ];
        let prompt = build_prompt_from_full_history("sys", &turns, 6000);
        assert!(prompt.contains("<|im_start|>tool\n[search] Found 2 results.\n<|im_end|>"));
    }

    #[test]
    fn estimate_tokens_rough() {
        // "hello world" = 11 chars → ~4 tokens
        let tokens = estimate_tokens("hello world");
        assert!(tokens >= 3 && tokens <= 5);
    }
}
