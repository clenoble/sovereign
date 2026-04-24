use serde::Serialize;

use crate::manifest::Capability;
use crate::markdown_util::strip_markdown_lite;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct ReadabilityScoreSkill;

#[derive(Debug, Serialize)]
struct ReadabilityResult {
    flesch_kincaid_grade: f32,
    gunning_fog: f32,
    coleman_liau: f32,
    words: usize,
    sentences: usize,
    syllables: usize,
    /// Words with 3+ syllables, used by Gunning Fog.
    complex_words: usize,
    /// Letter count (Coleman-Liau uses letters, not all chars).
    letters: usize,
}

impl CoreSkill for ReadabilityScoreSkill {
    fn name(&self) -> &str {
        "readability-score"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadDocument]
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
        _ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "score" => {
                let result = score(&doc.content.body);
                let json = serde_json::to_string(&result)?;
                Ok(SkillOutput::StructuredData {
                    kind: "readability".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("score".into(), "Score Readability".into())]
    }

    fn file_types(&self) -> Vec<String> {
        vec!["md".into(), "txt".into()]
    }
}

fn score(text: &str) -> ReadabilityResult {
    let stripped = strip_markdown_lite(text);
    let sentences = count_sentences(&stripped);
    let words: Vec<&str> = stripped.split_whitespace().collect();
    let word_count = words.len();
    let mut syllables = 0usize;
    let mut complex_words = 0usize;
    let mut letters = 0usize;
    for w in &words {
        let cleaned: String = w
            .chars()
            .filter(|c| c.is_alphabetic() || *c == '\'')
            .collect();
        if cleaned.is_empty() {
            continue;
        }
        let s = count_syllables(&cleaned);
        syllables += s;
        if s >= 3 {
            complex_words += 1;
        }
        letters += cleaned.chars().filter(|c| c.is_alphabetic()).count();
    }

    // Flesch-Kincaid Grade Level: 0.39*(W/S) + 11.8*(syll/W) - 15.59
    let fk = if word_count > 0 && sentences > 0 {
        0.39 * (word_count as f32 / sentences as f32)
            + 11.8 * (syllables as f32 / word_count as f32)
            - 15.59
    } else {
        0.0
    };

    // Gunning Fog: 0.4 * (W/S + 100 * complex/W)
    let gf = if word_count > 0 && sentences > 0 {
        0.4 * (word_count as f32 / sentences as f32
            + 100.0 * (complex_words as f32 / word_count as f32))
    } else {
        0.0
    };

    // Coleman-Liau: 0.0588*L - 0.296*S - 15.8 where L,S are per 100 words
    let cl = if word_count > 0 {
        let l = (letters as f32 / word_count as f32) * 100.0;
        let s = (sentences as f32 / word_count as f32) * 100.0;
        0.0588 * l - 0.296 * s - 15.8
    } else {
        0.0
    };

    ReadabilityResult {
        flesch_kincaid_grade: round1(fk),
        gunning_fog: round1(gf),
        coleman_liau: round1(cl),
        words: word_count,
        sentences,
        syllables,
        complex_words,
        letters,
    }
}

fn round1(x: f32) -> f32 {
    (x * 10.0).round() / 10.0
}

fn count_sentences(text: &str) -> usize {
    let mut n = 0usize;
    let mut prev_was_terminator = false;
    for c in text.chars() {
        let is_term = matches!(c, '.' | '!' | '?');
        if is_term && !prev_was_terminator {
            n += 1;
        }
        prev_was_terminator = is_term;
    }
    // If text is non-empty but has no terminator, count as 1 sentence
    if n == 0 && text.trim().chars().any(|c| c.is_alphanumeric()) {
        n = 1;
    }
    n
}

/// Heuristic syllable count: count vowel groups, drop trailing silent 'e',
/// add one back for endings like 'le' after a consonant. Minimum 1 for any
/// alphabetic word. Suitable for grade-level estimation, not phonetics.
fn count_syllables(word: &str) -> usize {
    let lower = word.to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut count = 0usize;
    let mut prev_vowel = false;
    for c in &chars {
        let v = matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y');
        if v && !prev_vowel {
            count += 1;
        }
        prev_vowel = v;
    }
    // silent 'e' at end (e.g. "make", "love")
    if chars.len() >= 2
        && chars[chars.len() - 1] == 'e'
        && !is_vowel(chars[chars.len() - 2])
    {
        count = count.saturating_sub(1);
    }
    // 'le' ending after consonant adds a syllable back ("table", "people")
    if chars.len() >= 3
        && chars[chars.len() - 2..] == ['l', 'e']
        && !is_vowel(chars[chars.len() - 3])
    {
        count += 1;
    }
    count.max(1)
}

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{dummy_ctx, make_doc};

    fn run(body: &str) -> serde_json::Value {
        let skill = ReadabilityScoreSkill;
        let doc = make_doc(body);
        let result = skill.execute("score", &doc, "", &dummy_ctx()).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            serde_json::from_str(&json).unwrap()
        } else {
            panic!("expected StructuredData");
        }
    }

    #[test]
    fn empty_text_returns_zeros() {
        let v = run("");
        assert_eq!(v["words"], 0);
        assert_eq!(v["sentences"], 0);
        assert_eq!(v["syllables"], 0);
    }

    #[test]
    fn simple_sentence_counts_correctly() {
        let v = run("The cat sat on the mat. The dog ran fast.");
        assert_eq!(v["sentences"], 2);
        assert_eq!(v["words"], 11);
    }

    #[test]
    fn syllable_estimate_reasonable() {
        let s = count_syllables("readability");
        assert!((4..=6).contains(&s), "readability syllables: {s}");
        assert_eq!(count_syllables("cat"), 1);
        assert_eq!(count_syllables("simple"), 2);
    }

    #[test]
    fn produces_grade_levels_for_real_paragraph() {
        let body = "The quick brown fox jumps over the lazy dog. \
                    Pack my box with five dozen liquor jugs. \
                    The five boxing wizards jump quickly.";
        let v = run(body);
        let fk = v["flesch_kincaid_grade"].as_f64().unwrap();
        let gf = v["gunning_fog"].as_f64().unwrap();
        let cl = v["coleman_liau"].as_f64().unwrap();
        assert!((-2.0..30.0).contains(&fk), "fk = {fk}");
        assert!((-2.0..30.0).contains(&gf), "gf = {gf}");
        assert!((-2.0..30.0).contains(&cl), "cl = {cl}");
    }
}
