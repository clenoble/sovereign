//! Model-agnostic prompt formatting.
//!
//! Defines the `PromptFormatter` trait so the orchestrator can work with
//! different LLM families (Qwen/ChatML, Mistral, Llama 3) without changing
//! prompt construction, history assembly, or tool-call parsing logic.

use super::context::{ChatRole, ChatTurn};

/// Supported prompt formats, selectable via `AiConfig.prompt_format`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptFormat {
    /// Qwen2.5 / Hermes — `<|im_start|>role\n...\n<|im_end|>`
    ChatML,
    /// Mistral v0.3 / Ministral — `[INST]...[/INST]`
    Mistral,
    /// Llama 3.x — `<|start_header_id|>role<|end_header_id|>`
    Llama3,
}

impl PromptFormat {
    /// Parse from config string. Returns `ChatML` for unknown values.
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "mistral" => Self::Mistral,
            "llama3" | "llama" => Self::Llama3,
            _ => Self::ChatML,
        }
    }
}

/// Trait for model-family-specific prompt formatting.
///
/// Implementations handle the token delimiters, tool-call syntax, and
/// tokenizer characteristics of each model family while keeping the
/// orchestrator logic model-agnostic.
pub trait PromptFormatter: Send + Sync {
    /// Format a single-turn system+user prompt ending with the assistant preamble.
    fn format_system_user(&self, system: &str, user: &str) -> String;

    /// Format a multi-turn conversation from history, ending with the assistant preamble.
    fn format_conversation(&self, system: &str, turns: &[ChatTurn]) -> String;

    /// Opening tag for tool calls in model output (e.g. `"<tool_call>"`).
    fn tool_call_open_tag(&self) -> &str;

    /// Closing tag for tool calls in model output (e.g. `"</tool_call>"`).
    fn tool_call_close_tag(&self) -> &str;

    /// Format a tool result for insertion into the conversation.
    fn format_tool_turn(&self, content: &str) -> String;

    /// Approximate characters-per-token ratio for history budget calculation.
    fn chars_per_token(&self) -> f64;

    /// Instruction block telling the model how to format tool calls.
    fn tool_call_format_instruction(&self) -> String;

    /// Wrap a tool-call JSON string in the model's expected tags (for few-shot examples).
    fn wrap_tool_call_example(&self, json: &str) -> String;
}

/// Detect the prompt format from a GGUF filename.
///
/// Inspects the lowercase filename for known model-family keywords.
/// Returns `ChatML` as the default if no pattern matches.
///
/// Examples:
///  - `"Ministral-8B-Instruct-2410-Q4_K_M.gguf"` → `Mistral`
///  - `"mistral-7b-instruct-v0.3.Q5_K_M.gguf"` → `Mistral`
///  - `"Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"` → `Llama3`
///  - `"Qwen2.5-3B-Instruct-Q4_K_M.gguf"` → `ChatML` (default)
pub fn detect_format_from_filename(filename: &str) -> PromptFormat {
    let lower = filename.to_lowercase();
    if lower.contains("mistral") || lower.contains("ministral") {
        PromptFormat::Mistral
    } else if lower.contains("llama") {
        PromptFormat::Llama3
    } else {
        // Qwen, Hermes, and anything unknown default to ChatML
        PromptFormat::ChatML
    }
}

/// Create a boxed formatter from a `PromptFormat` enum.
pub fn create_formatter(format: PromptFormat) -> Box<dyn PromptFormatter> {
    match format {
        PromptFormat::ChatML => Box::new(ChatMLFormatter),
        PromptFormat::Mistral => Box::new(MistralFormatter),
        PromptFormat::Llama3 => Box::new(Llama3Formatter),
    }
}

// ---------------------------------------------------------------------------
// ChatML (Qwen2.5 / Hermes)
// ---------------------------------------------------------------------------

pub struct ChatMLFormatter;

impl PromptFormatter for ChatMLFormatter {
    fn format_system_user(&self, system: &str, user: &str) -> String {
        format!(
            "<|im_start|>system\n{system}\n<|im_end|>\n\
             <|im_start|>user\n{user}\n<|im_end|>\n\
             <|im_start|>assistant\n"
        )
    }

    fn format_conversation(&self, system: &str, turns: &[ChatTurn]) -> String {
        let mut prompt = format!("<|im_start|>system\n{system}\n<|im_end|>\n");
        for turn in turns {
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

    fn tool_call_open_tag(&self) -> &str {
        "<tool_call>"
    }

    fn tool_call_close_tag(&self) -> &str {
        "</tool_call>"
    }

    fn format_tool_turn(&self, content: &str) -> String {
        // In ChatML, tool results use the "tool" role — the role tag is
        // handled by format_conversation, so we just return the content.
        content.to_string()
    }

    fn chars_per_token(&self) -> f64 {
        3.5
    }

    fn tool_call_format_instruction(&self) -> String {
        "To use a tool, you MUST output exactly this format (no markdown, no code fences):\n\
         <tool_call>\n\
         {\"name\": \"tool_name\", \"arguments\": {\"key\": \"value\"}}\n\
         </tool_call>\n\n\
         You can call one tool per turn. After seeing the result, either call another tool or give your final answer.\n\
         For create/rename/move actions, ALWAYS use the tool — never just describe the action in text.\n"
            .to_string()
    }

    fn wrap_tool_call_example(&self, json: &str) -> String {
        format!("<tool_call>\n{json}\n</tool_call>")
    }
}

// ---------------------------------------------------------------------------
// Mistral v0.3 / Ministral
// ---------------------------------------------------------------------------

pub struct MistralFormatter;

impl PromptFormatter for MistralFormatter {
    fn format_system_user(&self, system: &str, user: &str) -> String {
        // Mistral prepends system message into the first user turn.
        format!("<s>[INST] {system}\n\n{user} [/INST]")
    }

    fn format_conversation(&self, system: &str, turns: &[ChatTurn]) -> String {
        let mut prompt = String::from("<s>");
        let mut first_user = true;

        for turn in turns {
            match turn.role {
                ChatRole::User => {
                    if first_user {
                        // System message is prepended to first user message
                        prompt.push_str(&format!(
                            "[INST] {system}\n\n{} [/INST]",
                            turn.content
                        ));
                        first_user = false;
                    } else {
                        prompt.push_str(&format!("[INST] {} [/INST]", turn.content));
                    }
                }
                ChatRole::Assistant => {
                    prompt.push_str(&format!(" {}</s>", turn.content));
                }
                ChatRole::Tool => {
                    prompt.push_str(&format!(
                        "[TOOL_RESULTS] {{\"content\": \"{}\"}}[/TOOL_RESULTS]",
                        turn.content.replace('"', "\\\"")
                    ));
                }
            }
        }

        // If no user turns seen, prepend system as first instruction
        if first_user {
            prompt.push_str(&format!("[INST] {system} [/INST]"));
        }

        prompt
    }

    fn tool_call_open_tag(&self) -> &str {
        "[TOOL_CALLS]"
    }

    fn tool_call_close_tag(&self) -> &str {
        // Mistral tool calls end at EOG/EOS, not with a closing tag.
        // We use a newline boundary for parsing.
        "\n"
    }

    fn format_tool_turn(&self, content: &str) -> String {
        content.to_string()
    }

    fn chars_per_token(&self) -> f64 {
        4.0
    }

    fn tool_call_format_instruction(&self) -> String {
        "To use a tool, output this format:\n\
         [TOOL_CALLS] [{\"name\": \"tool_name\", \"arguments\": {\"key\": \"value\"}}]\n\n\
         You can call one tool per turn. After seeing the result, either call another tool or give your final answer.\n\
         For create/rename/move actions, ALWAYS use the tool — never just describe the action in text.\n"
            .to_string()
    }

    fn wrap_tool_call_example(&self, json: &str) -> String {
        format!("[TOOL_CALLS] [{json}]")
    }
}

// ---------------------------------------------------------------------------
// Llama 3.x
// ---------------------------------------------------------------------------

pub struct Llama3Formatter;

impl PromptFormatter for Llama3Formatter {
    fn format_system_user(&self, system: &str, user: &str) -> String {
        format!(
            "<|begin_of_text|>\
             <|start_header_id|>system<|end_header_id|>\n\n\
             {system}<|eot_id|>\
             <|start_header_id|>user<|end_header_id|>\n\n\
             {user}<|eot_id|>\
             <|start_header_id|>assistant<|end_header_id|>\n\n"
        )
    }

    fn format_conversation(&self, system: &str, turns: &[ChatTurn]) -> String {
        let mut prompt = format!(
            "<|begin_of_text|>\
             <|start_header_id|>system<|end_header_id|>\n\n\
             {system}<|eot_id|>"
        );
        for turn in turns {
            let role = match turn.role {
                ChatRole::User => "user",
                ChatRole::Assistant => "assistant",
                ChatRole::Tool => "ipython",
            };
            prompt.push_str(&format!(
                "<|start_header_id|>{role}<|end_header_id|>\n\n\
                 {}<|eot_id|>",
                turn.content
            ));
        }
        prompt.push_str(
            "<|start_header_id|>assistant<|end_header_id|>\n\n",
        );
        prompt
    }

    fn tool_call_open_tag(&self) -> &str {
        "<|python_tag|>"
    }

    fn tool_call_close_tag(&self) -> &str {
        "<|eom_id|>"
    }

    fn format_tool_turn(&self, content: &str) -> String {
        content.to_string()
    }

    fn chars_per_token(&self) -> f64 {
        4.0
    }

    fn tool_call_format_instruction(&self) -> String {
        "To use a tool, output this format:\n\
         {\"name\": \"tool_name\", \"arguments\": {\"key\": \"value\"}}\n\n\
         You can call one tool per turn. After seeing the result, either call another tool or give your final answer.\n\
         For create/rename/move actions, ALWAYS use the tool — never just describe the action in text.\n"
            .to_string()
    }

    fn wrap_tool_call_example(&self, json: &str) -> String {
        json.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chatml_single_turn() {
        let f = ChatMLFormatter;
        let prompt = f.format_system_user("You are helpful.", "Hello");
        assert!(prompt.starts_with("<|im_start|>system\n"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("<|im_start|>user\nHello\n<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn chatml_multi_turn() {
        let f = ChatMLFormatter;
        let turns = vec![
            ChatTurn { role: ChatRole::User, content: "hi".into() },
            ChatTurn { role: ChatRole::Assistant, content: "hello".into() },
        ];
        let prompt = f.format_conversation("sys", &turns);
        assert!(prompt.contains("<|im_start|>user\nhi\n<|im_end|>"));
        assert!(prompt.contains("<|im_start|>assistant\nhello\n<|im_end|>"));
        assert!(prompt.ends_with("<|im_start|>assistant\n"));
    }

    #[test]
    fn chatml_tool_call_tags() {
        let f = ChatMLFormatter;
        assert_eq!(f.tool_call_open_tag(), "<tool_call>");
        assert_eq!(f.tool_call_close_tag(), "</tool_call>");
    }

    #[test]
    fn mistral_single_turn() {
        let f = MistralFormatter;
        let prompt = f.format_system_user("You are helpful.", "Hello");
        assert!(prompt.starts_with("<s>[INST]"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("Hello"));
        assert!(prompt.ends_with("[/INST]"));
    }

    #[test]
    fn mistral_multi_turn() {
        let f = MistralFormatter;
        let turns = vec![
            ChatTurn { role: ChatRole::User, content: "hi".into() },
            ChatTurn { role: ChatRole::Assistant, content: "hello".into() },
            ChatTurn { role: ChatRole::User, content: "bye".into() },
        ];
        let prompt = f.format_conversation("sys", &turns);
        assert!(prompt.starts_with("<s>[INST] sys\n\nhi [/INST]"));
        assert!(prompt.contains(" hello</s>"));
        assert!(prompt.contains("[INST] bye [/INST]"));
    }

    #[test]
    fn mistral_tool_call_tags() {
        let f = MistralFormatter;
        assert_eq!(f.tool_call_open_tag(), "[TOOL_CALLS]");
    }

    #[test]
    fn llama3_single_turn() {
        let f = Llama3Formatter;
        let prompt = f.format_system_user("You are helpful.", "Hello");
        assert!(prompt.contains("<|start_header_id|>system<|end_header_id|>"));
        assert!(prompt.contains("You are helpful."));
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>"));
        assert!(prompt.contains("Hello"));
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn llama3_tool_turn_uses_ipython() {
        let f = Llama3Formatter;
        let turns = vec![
            ChatTurn { role: ChatRole::User, content: "search".into() },
            ChatTurn { role: ChatRole::Tool, content: "found 3 results".into() },
        ];
        let prompt = f.format_conversation("sys", &turns);
        assert!(prompt.contains("<|start_header_id|>ipython<|end_header_id|>"));
        assert!(prompt.contains("found 3 results"));
    }

    #[test]
    fn format_from_str() {
        assert_eq!(PromptFormat::from_str("chatml"), PromptFormat::ChatML);
        assert_eq!(PromptFormat::from_str("mistral"), PromptFormat::Mistral);
        assert_eq!(PromptFormat::from_str("llama3"), PromptFormat::Llama3);
        assert_eq!(PromptFormat::from_str("llama"), PromptFormat::Llama3);
        assert_eq!(PromptFormat::from_str("unknown"), PromptFormat::ChatML);
    }

    #[test]
    fn detect_format_from_gguf_filename() {
        // Mistral family
        assert_eq!(
            detect_format_from_filename("Ministral-8B-Instruct-2410-Q4_K_M.gguf"),
            PromptFormat::Mistral,
        );
        assert_eq!(
            detect_format_from_filename("mistral-7b-instruct-v0.3.Q5_K_M.gguf"),
            PromptFormat::Mistral,
        );

        // Llama family
        assert_eq!(
            detect_format_from_filename("Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf"),
            PromptFormat::Llama3,
        );
        assert_eq!(
            detect_format_from_filename("llama-3.2-1b-instruct.Q8_0.gguf"),
            PromptFormat::Llama3,
        );

        // ChatML (default) — Qwen, Hermes, unknown
        assert_eq!(
            detect_format_from_filename("Qwen2.5-3B-Instruct-Q4_K_M.gguf"),
            PromptFormat::ChatML,
        );
        assert_eq!(
            detect_format_from_filename("Hermes-3-Llama-3.1-8B-Q4_K_M.gguf"),
            PromptFormat::Llama3, // Hermes on Llama base → detects "llama"
        );
        assert_eq!(
            detect_format_from_filename("some-unknown-model.gguf"),
            PromptFormat::ChatML,
        );
    }

    #[test]
    fn wrap_tool_call_example_formats() {
        let json = r#"{"name": "search_documents", "arguments": {"query": "test"}}"#;

        let chatml = ChatMLFormatter;
        assert!(chatml.wrap_tool_call_example(json).contains("<tool_call>"));

        let mistral = MistralFormatter;
        assert!(mistral.wrap_tool_call_example(json).contains("[TOOL_CALLS]"));

        let llama = Llama3Formatter;
        // Llama3 uses bare JSON
        assert_eq!(llama.wrap_tool_call_example(json), json);
    }
}
