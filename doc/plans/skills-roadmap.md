# Skills Roadmap

Plan for closing existing skill-system gaps, implementing high-value core skills, and seeding community contributions for the rest. Builds on the WASM Component Model plugin runtime delivered in commit `1606097` and the third-party developer guide at [../writing-skills.md](../writing-skills.md).

Current state (as of this draft): 10 core Rust skills registered in [../../crates/sovereign-skills/src/registry.rs](../../crates/sovereign-skills/src/registry.rs), 1 WASM example at [../../skills/word-count-wasm/](../../skills/word-count-wasm/). Two coherence gaps in the existing scaffolding are addressed in Phase 1.

---

## Phase 1 — Gap fixes

Small, clear cleanup before any new work.

- [ ] **Delete the orphaned summarizer manifest.** [../../skills/summarizer/](../../skills/summarizer/) declares a "Summarizer" skill with no implementation in either `src/skills/` or as a `.wasm` component. Its role is absorbed by Thread Summary in Wave E — at thread scope rather than single-doc.
- [ ] **Reconcile image / image-viewer naming.** Manifest at [../../skills/image-viewer/skill.json](../../skills/image-viewer/skill.json) uses identifier `image-viewer`; impl at [../../crates/sovereign-skills/src/skills/image.rs](../../crates/sovereign-skills/src/skills/image.rs) registers as `image`. Rename the manifest directory `skills/image-viewer/` → `skills/image/`. Display name in `skill.json` may stay "Image Viewer".

---

## Phase 2 — Infrastructure prerequisites

Two pieces of plumbing that the core skill list needs. Both are larger than any individual skill in Phase 3 and shape the architecture going forward.

### 2a. Extend `SkillDbAccess` with relationship queries

Required by Backlink Map and Orphan Finder (Wave D).

The trait at [../../crates/sovereign-skills/src/traits.rs#L36-L45](../../crates/sovereign-skills/src/traits.rs#L36-L45) currently only exposes `search_documents`, `get_document`, `list_documents`, `create_document`. Add:

- `list_relationships(doc_id) -> Vec<(relationship_type, target_id)>` — outgoing edges.
- `list_backlinks(doc_id) -> Vec<(source_id, relationship_type)>` — incoming edges.
- `list_all_documents_with_link_counts() -> Vec<(id, title, in_degree, out_degree)>` — supports Orphan Finder without N+1 queries.

Implement in [../../crates/sovereign-skills/src/db_bridge.rs](../../crates/sovereign-skills/src/db_bridge.rs).

**WIT host-db not extended for now.** These three methods are core-only until a WASM community skill needs them.

### 2b. Expose LLM access to skills

Required by Thread Summary (Wave E).

- Add `Capability::LlmInference` to the Rust enum in [../../crates/sovereign-skills/src/manifest.rs#L14-L22](../../crates/sovereign-skills/src/manifest.rs#L14-L22).
- Define a new trait `SkillLlmAccess` in [../../crates/sovereign-skills/src/traits.rs](../../crates/sovereign-skills/src/traits.rs) mirroring the `SkillDbAccess` pattern. Minimal surface: a single `generate(prompt, max_tokens) -> String` to start; refine if needed.
- Extend `SkillContext` with `pub llm: Option<Arc<dyn SkillLlmAccess>>`.
- Wire the orchestrator into the bridge in [../../crates/sovereign-app/](../../crates/sovereign-app/) so registered skills receive a working `SkillLlmAccess` impl.

**WIT capability not extended.** Exposing LLM inference to WASM is its own non-trivial work (streaming across the component boundary, fuel-limit interaction with multi-second LLM calls, model selection from the sandbox). Defer until a WASM skill needs it.

---

## Phase 3 — Core skills

14 skills + 2 markdown-editor commands. Sequenced by capability complexity and infra dependency. Each wave can be a separate PR.

### Wave A — `read_document` only

No infra prerequisites. Output is `StructuredData` or `File`. Good shake-down for the skill scaffolding.

| Skill | Action(s) | Capability | Output | File types | Notes |
|---|---|---|---|---|---|
| **Outline Extractor** | `extract` | `read_document` | StructuredData (`outline`) | md, markdown | Heading tree as nested JSON. |
| **Readability Score** | `score` | `read_document` | StructuredData (`readability`) | md, txt | Flesch-Kincaid + Gunning Fog + Coleman-Liau. No external dictionaries. |
| **Link Checker** | `extract` | `read_document` | StructuredData (`links`) | md, markdown | Extracts URLs, no fetching (network would require `network` capability). |
| **PII Detector** | `scan` | `read_document` | StructuredData (`pii_findings`) | md, txt | Detection rules in a shared internal module — Redactor in Wave B reuses it. |
| **HTML Export** | `export` | `read_document` | File (`text/html`) | md, markdown | Single-file standalone HTML, inline CSS. |
| **Plaintext Export** | `export` | `read_document` | File (`text/plain`) | md, markdown | Strip all markdown formatting. |

### Wave B — `read_document` + `write_document`

Adds `ContentUpdate` output. Redactor depends on PII Detector's detection module from Wave A.

| Skill | Action(s) | Capability | Output | File types | Notes |
|---|---|---|---|---|---|
| **Table of Contents** | `insert`, `update` | `read_document`, `write_document` | ContentUpdate | md, markdown | Inserts/refreshes a TOC marker block from headings. |
| **JSON/YAML Formatter** | `format`, `minify` | `read_document`, `write_document` | ContentUpdate | md, json, yaml | Pretty-print or minify fenced code blocks. |
| **CSV → Markdown Table** | `convert` | `read_document`, `write_document` | ContentUpdate | md, csv | Converts CSV blocks (or whole CSV file) into markdown tables. |
| **Redactor** | `redact` | `read_document`, `write_document` | ContentUpdate | md, txt | Replaces PII findings with `[REDACTED]`. Shares detection module with PII Detector. |

### Wave C — markdown-editor commands

Land inside [../../crates/sovereign-skills/src/skills/markdown_editor.rs](../../crates/sovereign-skills/src/skills/markdown_editor.rs) as new actions exposed via `actions()`. No new skill registrations.

- [ ] **Sort Lines** — alphabetic sort of selected lines or list items.
- [ ] **Case Converter** — Title / UPPER / lower / camelCase / snake_case. Action takes a `case` parameter via `params`.

### Wave D — `read_all_documents` / `write_all_documents`

Needs Phase 2a (relationship queries). Daily Journal needs a thread-bootstrap behavior.

| Skill | Action(s) | Capability | Output | File types | Notes |
|---|---|---|---|---|---|
| **Backlink Map** | `find_backlinks` | `read_all_documents` | StructuredData (`backlinks`) | (universal) | Lists docs that reference the current one. Uses `list_backlinks` from Phase 2a. |
| **Orphan Finder** | `find_orphans` | `read_all_documents` | StructuredData (`orphans`) | (universal) | Lists docs with no incoming links. Uses `list_all_documents_with_link_counts` from Phase 2a. |
| **Daily Journal** | `today` | `write_all_documents` | StructuredData (`journal_entry`) | (universal) | Auto-creates a "Journal" thread on first invocation if none exists. Creates a date-stamped doc (`YYYY-MM-DD`); if today's already exists, returns its id rather than duplicating. |

### Wave E — LLM-using

Needs Phase 2b (`SkillLlmAccess` + `Capability::LlmInference`). Last because the infra it depends on is the most invasive.

| Skill | Action(s) | Capability | Output | File types | Notes |
|---|---|---|---|---|---|
| **Thread Summary** | `summarize_thread` | `read_all_documents`, `llm_inference` | StructuredData (`thread_summary`) | (universal) | Bullet-point summary of all docs in the current thread. Replaces the deleted single-doc summarizer; the design choice is that thread-scope summaries are useful in a way single-doc summaries are not. |

---

## Phase 4 — Community spec-as-seeds

For each entry below, create a directory under [../../skills/](../../skills/) containing:
- `skill.json` — manifest stub with proposed name, version `0.0.1`, `skill_type: "community"`, capabilities, file_types.
- `SPEC.md` — see template below.

No implementation. The point is to make community contribution low-friction by leaving a complete contract in place.

### SPEC.md template

```markdown
# <Skill Name>

## Purpose
One paragraph: what this skill does, who would use it, what need it addresses.

## Capabilities
List of `Capability` values from the WIT enum, with one-line justification each.

## Actions
| action_id | display label | params (JSON schema or "") | output variant |
|---|---|---|---|
| ... | ... | ... | ... |

## Sample input → output
Concrete example with realistic document body and the expected output payload.

## Complexity estimate
Small / Medium / Large, with a sentence on the dominant cost (algorithm, dependency size, host-db calls).

## Host-db extensions needed
None / list of WIT host-db functions that would need to be added (cross-reference Phase 2 discussion if relevant).

## Suggested implementation notes
Optional. Library suggestions, edge cases, gotchas worth flagging to a contributor.
```

### Targets (20 specs)

**Formatting & Style (4):**
- Remove Duplicates
- Wrap/Unwrap Lines
- Strip Comments
- Footnote Collector

**Document Automation (4):**
- Meeting Notes Template
- Merge Documents
- Template Stamp
- Changelog Generator

**Text & Writing (4):**
- Grammar Check
- Spell Check
- Slug Generator
- Lorem Ipsum

**Data & Analysis (3):**
- Frontmatter Parser
- Duplicate Finder
- Tag Extractor

**Export & Conversion (3):**
- EPUB Export
- LaTeX Export
- Slides Export

**Security & Privacy (1):**
- Entropy Scanner

**Cross-Document (1):**
- Word Frequency

---

## Sequencing summary

```
Phase 1 (gap fixes) ──┐
                      ├─→ Phase 3 Wave A (6 skills, no infra)
                      ├─→ Phase 3 Wave B (4 skills) ──→ Wave C (markdown-editor commands)
Phase 2a (db access) ─┴─→ Phase 3 Wave D (3 skills)
Phase 2b (llm access) ───→ Phase 3 Wave E (1 skill)
                          │
                          └─→ Phase 4 (20 community specs, parallel to anything)
```

Phase 4 is independent of every other phase — specs can be written at any point, including during Phase 3 if energy fits the writing register better than the implementation register.
