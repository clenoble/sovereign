//! System prompt builders for the Sovereign GE orchestrator.
//!
//! Replaces bare constant strings with rich, context-aware prompt builders
//! that include the Sovereign GE identity, few-shot examples, UX principles,
//! workspace context, and tool definitions.

use super::context::WorkspaceContext;
use super::format::PromptFormatter;
use crate::tools::format_tool_descriptions;

/// Core identity shared across all prompts.
const SOVEREIGN_IDENTITY: &str = "\
You are the AI assistant for Sovereign GE, a local-first personal graphic environment. \
Sovereign GE organizes the user's documents, threads (projects), contacts, and \
conversations on their own device. Everything is private and local — no cloud, no \
external servers. You help the user navigate, search, organize, and understand \
their workspace.";

/// Build a single-turn system+user prompt using the given formatter.
pub fn format_single_turn(formatter: &dyn PromptFormatter, system: &str, user: &str) -> String {
    formatter.format_system_user(system, user)
}

/// Build a Qwen2.5 chat-format prompt with system + user messages (single-turn).
/// Retained for backward compatibility — delegates to ChatML formatter.
pub fn qwen_chat_prompt(system: &str, user: &str) -> String {
    let f = super::format::ChatMLFormatter;
    f.format_system_user(system, user)
}

/// Build the router (3B) system prompt with few-shot examples.
pub fn build_router_system_prompt() -> String {
    format!(
        "{SOVEREIGN_IDENTITY}\n\n\
Your task: classify the user's input into an action. Output JSON only, no other text.\n\
Format: {{\"action\": \"...\", \"target\": \"...\", \"confidence\": 0.0-1.0, \"entities\": []}}\n\n\
Actions:\n\
- search: find documents by keyword\n\
- open: open a specific document\n\
- create_document: create a new document\n\
- create_thread: create a new thread (project)\n\
- rename_thread: rename an existing thread\n\
- delete_thread: delete a thread\n\
- move_document: move a document to a different thread\n\
- history: show version history of a document\n\
- restore: restore a document to a previous version\n\
- summarize: summarize a document's content\n\
- adopt: mark an external document as owned\n\
- create_milestone: create a milestone on a thread timeline\n\
- list_milestones: list milestones for a thread\n\
- merge_threads: merge two threads\n\
- split_thread: split documents out of a thread into a new one\n\
- list_contacts: list all contacts\n\
- view_messages: view messages in a conversation\n\
- list_models: list available AI models\n\
- swap_model: switch to a different AI model\n\
- chat: general conversation, questions, or requests needing a detailed response\n\
- unknown: cannot determine intent\n\n\
Examples:\n\
User: find my meeting notes\n\
{{\"action\": \"search\", \"target\": \"meeting notes\", \"confidence\": 0.95, \"entities\": []}}\n\n\
User: open the budget document\n\
{{\"action\": \"open\", \"target\": \"budget\", \"confidence\": 0.92, \"entities\": []}}\n\n\
User: create a new thread called Prototyping\n\
{{\"action\": \"create_thread\", \"target\": \"Prototyping\", \"confidence\": 0.98, \"entities\": []}}\n\n\
User: move the API Spec to Development\n\
{{\"action\": \"move_document\", \"target\": \"API Spec\", \"confidence\": 0.90, \"entities\": [[\"doc\", \"API Spec\"], [\"thread\", \"Development\"]]}}\n\n\
User: what documents do I have about architecture?\n\
{{\"action\": \"chat\", \"target\": null, \"confidence\": 0.85, \"entities\": [[\"topic\", \"architecture\"]]}}\n\n\
User: how's the weather today?\n\
{{\"action\": \"chat\", \"target\": null, \"confidence\": 0.95, \"entities\": []}}\n\n\
User: rename thread Alpha to Beta\n\
{{\"action\": \"rename_thread\", \"target\": \"Alpha\", \"confidence\": 0.95, \"entities\": [[\"old_name\", \"Alpha\"], [\"new_name\", \"Beta\"]]}}\n\n\
User: delete the old drafts thread\n\
{{\"action\": \"delete_thread\", \"target\": \"old drafts\", \"confidence\": 0.88, \"entities\": []}}\n\n\
User: switch to Ministral\n\
{{\"action\": \"swap_model\", \"target\": \"Ministral\", \"confidence\": 0.95, \"entities\": []}}\n\n\
User: use the llama model\n\
{{\"action\": \"swap_model\", \"target\": \"llama\", \"confidence\": 0.92, \"entities\": []}}\n\n\
User: what models are available?\n\
{{\"action\": \"list_models\", \"target\": null, \"confidence\": 0.95, \"entities\": []}}"
    )
}

/// Build the reasoning (7B) system prompt with few-shot examples.
pub fn build_reasoning_system_prompt() -> String {
    format!(
        "{SOVEREIGN_IDENTITY}\n\n\
Analyze the user's request carefully and output JSON with a reasoning field.\n\
Format: {{\"action\": \"...\", \"target\": \"...\", \"confidence\": 0.0-1.0, \"entities\": [], \"reasoning\": \"...\"}}\n\n\
Actions: search, open, create_document, create_thread, rename_thread, delete_thread, \
move_document, history, restore, summarize, adopt, create_milestone, list_milestones, \
merge_threads, split_thread, list_contacts, view_messages, list_models, swap_model, chat, unknown\n\n\
Examples:\n\
User: I need to reorganize my API docs into the dev project\n\
{{\"action\": \"move_document\", \"target\": \"API docs\", \"confidence\": 0.85, \
\"entities\": [[\"doc\", \"API docs\"], [\"thread\", \"dev\"]], \
\"reasoning\": \"User wants to move API-related documents to the development thread.\"}}\n\n\
User: what did Alice say about the architecture last week?\n\
{{\"action\": \"chat\", \"target\": null, \"confidence\": 0.90, \
\"entities\": [[\"contact\", \"Alice\"], [\"topic\", \"architecture\"]], \
\"reasoning\": \"User is asking about message history with Alice regarding architecture. Requires searching messages and conversation context.\"}}"
    )
}

/// Build the chat system prompt with workspace context, tools, and UX principles.
pub fn build_chat_system_prompt(
    ctx: Option<&WorkspaceContext>,
    verbosity: &str,
    user_name: Option<&str>,
    designation: Option<&str>,
    nickname: Option<&str>,
    formatter: Option<&dyn PromptFormatter>,
) -> String {
    // Default to ChatML if no formatter provided (backward compat).
    let default_fmt = super::format::ChatMLFormatter;
    let fmt: &dyn PromptFormatter = formatter.unwrap_or(&default_fmt);
    let mut prompt = String::from(SOVEREIGN_IDENTITY);
    prompt.push_str("\n\n");

    // AI identity
    if let Some(desig) = designation {
        prompt.push_str(&format!("Your designation is {desig}. "));
        if let Some(nick) = nickname {
            prompt.push_str(&format!("The user calls you {nick}.\n"));
        } else {
            prompt.push_str("The user may call you by a short nickname.\n");
        }
    }

    // Personality based on verbosity preference
    match verbosity {
        "terse" => prompt.push_str("Be brief and direct. Use short sentences. Skip pleasantries.\n"),
        "conversational" => {
            prompt.push_str("Be warm and conversational. Use a friendly, natural tone.\n")
        }
        _ => prompt.push_str("Be clear and helpful. Give concise but complete answers.\n"),
    }

    if let Some(name) = user_name {
        prompt.push_str(&format!("The user's name is {name}.\n"));
    }

    // Condensed UX principles
    prompt.push_str(
        "\nRules:\n\
         - For write actions (create, rename, move), always use the appropriate tool. The system will ask the user for confirmation automatically.\n\
         - Label content as (owned) or (external) when reporting results.\n\
         - For multi-step tasks, state your plan first.\n\
         - Rank multiple matches by relevance. When uncertain, say so. Never say \"I can't\" without suggesting an alternative.\n\
         - You can create documents, threads, rename threads, and move documents using write tools.\n",
    );

    // Workspace context
    if let Some(ctx) = ctx {
        prompt.push_str("\nCurrent workspace:\n");
        prompt.push_str(&super::context::format_workspace_context(ctx));
    }

    // Tool definitions (format-aware)
    prompt.push('\n');
    prompt.push_str(&format_tool_descriptions(fmt));

    // Few-shot examples using formatter's wrapping
    let ex1 = fmt.wrap_tool_call_example(r#"{"name": "list_documents", "arguments": {"thread": "Research"}}"#);
    let ex2 = fmt.wrap_tool_call_example(r#"{"name": "create_document", "arguments": {"title": "Project Ideas"}}"#);
    let ex3 = fmt.wrap_tool_call_example(r#"{"name": "create_thread", "arguments": {"name": "Marketing"}}"#);

    prompt.push_str(&format!(
        "\nExamples:\n\
         User: what documents do I have in Research?\n\
         {ex1}\n\n\
         [After receiving tool results, respond naturally with the information, noting provenance.]\n\n\
         User: create a document called Project Ideas\n\
         {ex2}\n\n\
         [The system asks the user for confirmation. After approval, respond naturally confirming the action.]\n\n\
         User: create a new thread called Marketing\n\
         {ex3}\n\n\
         [The system asks the user for confirmation. After approval, respond naturally confirming the action.]\n\n\
         User: hello!\n\
         Hello! How can I help you with your workspace today?\n",
    ));

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qwen_chat_prompt_format() {
        let prompt = qwen_chat_prompt("You are helpful.", "Hello");
        assert!(prompt.starts_with("<|im_start|>system\n"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("<|im_start|>user\nHello\n<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn router_prompt_contains_all_actions() {
        let prompt = build_router_system_prompt();
        let actions = [
            "search", "open", "create_document", "create_thread", "rename_thread",
            "delete_thread", "move_document", "history", "restore", "summarize",
            "adopt", "create_milestone", "list_milestones", "merge_threads",
            "split_thread", "list_contacts", "view_messages", "list_models",
            "swap_model", "chat", "unknown",
        ];
        for action in actions {
            assert!(prompt.contains(action), "Missing action: {action}");
        }
    }

    #[test]
    fn router_prompt_contains_few_shot_examples() {
        let prompt = build_router_system_prompt();
        assert!(prompt.contains("find my meeting notes"));
        assert!(prompt.contains("\"action\": \"search\""));
        assert!(prompt.contains("open the budget document"));
        assert!(prompt.contains("\"action\": \"open\""));
    }

    #[test]
    fn reasoning_prompt_has_reasoning_field() {
        let prompt = build_reasoning_system_prompt();
        assert!(prompt.contains("\"reasoning\""));
        assert!(prompt.contains("Analyze the user's request carefully"));
    }

    #[test]
    fn chat_prompt_respects_terse_verbosity() {
        let prompt = build_chat_system_prompt(None, "terse", None, None, None, None);
        assert!(prompt.contains("brief and direct"));
    }

    #[test]
    fn chat_prompt_respects_conversational_verbosity() {
        let prompt = build_chat_system_prompt(None, "conversational", None, None, None, None);
        assert!(prompt.contains("warm and conversational"));
    }

    #[test]
    fn chat_prompt_includes_user_name() {
        let prompt = build_chat_system_prompt(None, "detailed", Some("Alex"), None, None, None);
        assert!(prompt.contains("Alex"));
    }

    #[test]
    fn chat_prompt_includes_workspace_context() {
        let ctx = WorkspaceContext {
            thread_count: 4,
            doc_count: 14,
            thread_names: vec!["Research".into(), "Development".into()],
            recent_doc_titles: vec!["Project Plan".into()],
            contact_count: 5,
            unread_conversations: 1,
        };
        let prompt = build_chat_system_prompt(Some(&ctx), "detailed", None, None, None, None);
        assert!(prompt.contains("4 threads"));
        assert!(prompt.contains("Research, Development"));
        assert!(prompt.contains("Project Plan"));
    }

    #[test]
    fn chat_prompt_includes_tools() {
        let prompt = build_chat_system_prompt(None, "detailed", None, None, None, None);
        assert!(prompt.contains("search_documents"));
        assert!(prompt.contains("list_threads"));
        assert!(prompt.contains("<tool_call>"));
    }

    #[test]
    fn chat_prompt_includes_ux_principles() {
        let prompt = build_chat_system_prompt(None, "detailed", None, None, None, None);
        // Principle 2: Conversational Confirmation
        assert!(prompt.contains("confirmation"));
        // Principle 3: Provenance
        assert!(prompt.contains("owned"));
        assert!(prompt.contains("external"));
        // Principle 4: Plan Visibility
        assert!(prompt.contains("plan"));
        // Principle 8: Error & Uncertainty
        assert!(prompt.contains("Rank"));
    }

    #[test]
    fn chat_prompt_includes_write_tool_examples() {
        let prompt = build_chat_system_prompt(None, "detailed", None, None, None, None);
        assert!(prompt.contains("create_document"));
        assert!(prompt.contains("\"name\": \"create_document\""));
        assert!(prompt.contains("\"name\": \"create_thread\""));
        assert!(prompt.contains("Project Ideas"));
    }

    #[test]
    fn chat_prompt_includes_designation() {
        let prompt = build_chat_system_prompt(
            None, "detailed", None, Some("Ikshal-B4T9-Ω"), None, None,
        );
        assert!(prompt.contains("Ikshal-B4T9-Ω"));
        assert!(prompt.contains("designation"));
    }

    #[test]
    fn chat_prompt_includes_designation_and_nickname() {
        let prompt = build_chat_system_prompt(
            None, "detailed", None, Some("Ikshal-B4T9-Ω"), Some("Ike"), None,
        );
        assert!(prompt.contains("Ikshal-B4T9-Ω"));
        assert!(prompt.contains("Ike"));
    }

    #[test]
    fn chat_prompt_omits_designation_when_none() {
        let prompt = build_chat_system_prompt(None, "detailed", None, None, None, None);
        assert!(!prompt.contains("designation"));
    }
}
