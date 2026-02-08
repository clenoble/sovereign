/// Build a Qwen2.5 chat-format prompt with system + user messages.
pub fn qwen_chat_prompt(system: &str, user: &str) -> String {
    format!(
        "<|im_start|>system\n{system}\n<|im_end|>\n\
         <|im_start|>user\n{user}\n<|im_end|>\n\
         <|im_start|>assistant\n"
    )
}

/// System prompt for the intent classifier (3B router).
pub const ROUTER_SYSTEM_PROMPT: &str = "\
You are an intent classifier for a document management OS. \
Given user input, output JSON only:\n\
{\"action\": \"search|open|create|navigate|summarize|unknown\", \
\"target\": \"...\", \"confidence\": 0.0-1.0, \
\"entities\": [[\"key\", \"value\"]]}";

/// System prompt for the reasoning model (7B escalation).
pub const REASONING_SYSTEM_PROMPT: &str = "\
You are a helpful assistant for a document management OS. \
Analyze the user's request carefully and output JSON:\n\
{\"action\": \"search|open|create|navigate|summarize|unknown\", \
\"target\": \"...\", \"confidence\": 0.0-1.0, \
\"entities\": [[\"key\", \"value\"]], \
\"reasoning\": \"brief explanation\"}";

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
    fn router_system_prompt_contains_actions() {
        assert!(ROUTER_SYSTEM_PROMPT.contains("search"));
        assert!(ROUTER_SYSTEM_PROMPT.contains("open"));
        assert!(ROUTER_SYSTEM_PROMPT.contains("create"));
        assert!(ROUTER_SYSTEM_PROMPT.contains("navigate"));
        assert!(ROUTER_SYSTEM_PROMPT.contains("summarize"));
        assert!(ROUTER_SYSTEM_PROMPT.contains("unknown"));
    }
}
