# 11 — Skills

Skills replace monolithic applications: small, composable operations on documents in the graph.
This chapter defines the skill API as built — core skills compiled into the binary, a WASM
sandbox for third-party skills — what each may touch, and how the community contributes.

*Consolidates:* specification v1.3 §Skill API Specification, Open Questions 6 and 8 (community
contribution); archived plan `skills-roadmap.md`; `doc/writing-skills.md`; code in
`crates/sovereign-skills`, `skills/`, and the skill wiring in `crates/sovereign-app` and
`crates/sovereign-shell`.

**Author guide.** How to write, build and install a third-party skill is in
[`doc/writing-skills.md`](../writing-skills.md). This chapter does not repeat it; where the two
differ on what ships, this chapter holds.

---

## Philosophy

**Status:** ◐ Partial — the trust split (core in-process, third-party sandboxed) is built; the
audit and signing process around third-party skills is not.

Skills are trusted, audited components — closer to kernel modules than app-store apps.

- **Core skills** ship with the binary, are maintained and audited with it, and run
  in-process.
- **Third-party skills** run in a WebAssembly sandbox with capability-checked host access and
  hard resource limits.
- Code transparency: skills are open source; community audit before inclusion (○).
- Global approval once installed, no per-document permission prompts (carried from v1.x).

---

## Core skills

**Status:** ◐ Partial — 24 core skills built; registered in the native shell and the mobile
backend. The shell runs only each skill's first action, with no parameters, database or model
access; the mobile app has no skill UI (code: `crates/sovereign-skills/src/skills/`).

| Skill | Principal actions | Capabilities |
|---|---|---|
| text-editor | save | read_document, write_document |
| markdown-editor | normalize, sort_lists, convert_case, preview | read_document, write_document |
| find-replace | find_replace | read_document, write_document |
| table-of-contents | insert, update | read_document, write_document |
| csv-to-md | convert | read_document, write_document |
| json-yaml-formatter | format, minify | read_document, write_document |
| redactor | redact | read_document, write_document |
| image | add, remove | read_document, write_document, read_filesystem |
| video | add, remove, play | read_document, write_document |
| word-count | count | read_document |
| readability-score | score | read_document |
| outline-extractor | extract | read_document |
| link-checker | extract | read_document |
| pii-detector | scan | read_document |
| html-export | export | read_document |
| plaintext-export | export | read_document |
| pdf-export | export | read_document, write_filesystem |
| search | search | read_all_documents |
| backlink-map | find_backlinks | read_all_documents |
| orphan-finder | find_orphans | read_all_documents |
| duplicate-document | duplicate | read_all_documents, write_all_documents |
| daily-journal | today | write_all_documents |
| file-import | import | read_filesystem, write_all_documents |
| thread-summary | summarize_thread | read_all_documents, llm_inference |

Design choices carried by specific skills:

- **Thread Summary replaces the single-document summarizer.** It summarizes all documents of
  the current document's thread in bullet points (per-document and total prompt caps; truncated
  documents are reported). Thread-scope summaries are useful where single-document summaries are
  not. The orchestrator's own `summarize` intent (ch. 08) remains for one document.
- **Link Checker never fetches.** It extracts Markdown links, autolinks and bare URLs and
  reports them; fetching would require the `network` capability.
- **Redactor shares PII Detector's detection rules** (one internal module, `pii_detector`),
  replacing matches with `[REDACTED]`. These rules are the skills' own; the PII pipeline of
  ch. 09 is separate.
- **Daily Journal** creates a "Journal" thread on first use and one dated document per day,
  returning the existing one rather than duplicating it.

*Replaces v1.x:* "8 core skills" (text editor, image, PDF export, word count, find & replace,
search, file import, duplicate document) — 24 are built.

### The `CoreSkill` trait

**Status:** ✅ Built (code: `crates/sovereign-skills/src/traits.rs`).

```rust
pub trait CoreSkill: Send + Sync {
    fn name(&self) -> &str;
    fn required_capabilities(&self) -> Vec<Capability>;
    fn activate(&mut self) -> anyhow::Result<()>;
    fn deactivate(&mut self) -> anyhow::Result<()>;
    fn execute(&self, action: &str, doc: &SkillDocument, params: &str,
               ctx: &SkillContext) -> anyhow::Result<SkillOutput>;
    fn actions(&self) -> Vec<(String, String)>;      // (action_id, display label)
    fn file_types(&self) -> Vec<String> { vec![] }   // empty = universal
}
```

- **Input:** `SkillDocument` — id, title, parsed `ContentFields` (body, images, …).
- **Output:** `SkillOutput` — `ContentUpdate(ContentFields)` to save back, `File { name,
  mime_type, data }`, `StructuredData { kind, json }`, or `None`.
- **Context:** `SkillContext { granted, db, llm }`. The registry refuses to run a skill whose
  required capabilities are not all in `granted`.

Calls are direct Rust trait calls — no IPC, no serialization.

### Narrow host interfaces

**Status:** ◐ Partial — built and wired in the Tauri backend (frozen on desktop; on mobile no
UI calls it); the native shell passes no database or LLM access, so skills that need them fail
there (ch. 12) (code:
`crates/sovereign-skills/src/traits.rs`, `db_bridge.rs`, `crates/sovereign-app/src/llm_bridge.rs`).

Skills never see the whole database or the orchestrator.

**`SkillDbAccess`** — the only database surface:

| Method | Returns |
|---|---|
| `search_documents(query)` | (id, title, snippet) |
| `get_document(id)` | (title, thread_id, content) |
| `list_documents(thread_id?)` | (id, title) |
| `create_document(title, thread_id, content)` | id |
| `list_relationships(doc_id)` | (relation_type, target_id) — outgoing |
| `list_backlinks(doc_id)` | (source_id, relation_type) — incoming |
| `list_all_documents_with_link_counts()` | (id, title, in_degree, out_degree) |
| `find_or_create_thread(name, description)` | thread id |

**Relationship queries are core-only**: the three relationship methods are not exposed to WASM
skills until a third-party skill needs them.

**`SkillLlmAccess`** — a single call, `generate(prompt, max_tokens) -> String`, bridged to the
orchestrator's loaded router model. Requires the `llm_inference` capability.

**Requirement:** content a skill sends to the model receives the same protections as the
orchestrator's own content paths (fencing of untrusted text, PII redaction — ch. 08, ch. 09).
**Status:** ○ Specified, not built.

### Invocation

**Status:** ◐ Partial — shell: right-click → Skills lists all 24 skills, not filtered by file
type, and runs each skill's first action with empty parameters; skills that need input do not
work. Mobile: no skill UI (code: `crates/sovereign-shell/src/app.rs`).

Skills are invoked by a direct user gesture. On the desktop, the native shell's document card
menu (right-click → Skills) lists every registered skill under the label of its first action
and runs that action; other actions, and parameter input (find-replace, search, file-import,
adding an image or video), are not reachable. The mobile app has no skill UI. The frozen Tauri
desktop app had a Skills panel filtered by the document's file type; it is not shipped.

The context grants exactly the capabilities the skill declares, so the capability check guards
against a skill using more than it declared, not against what it declared.

**Requirement:** a skill that rewrites a document's content never loses content it could not
read. **Status:** ◐ Partial — skills that rewrite content expect the structured content
encoding; documents stored as raw text are misread and running such a skill can overwrite
their body. This is a known defect, tracked in the backlog; it depends on the content-encoding
decision (ch. 03, *Content encoding*).

The orchestrator does not execute skills: an intent such as `word_count` or `find_replace`
returns a hint naming the skill to use from the skills panel — on the desktop, the shell's
Skills menu.

---

## Capabilities

**Status:** ✅ Built (code: `crates/sovereign-skills/src/manifest.rs`).

| Capability | Meaning |
|---|---|
| `read_document` | Read the document the skill was invoked on |
| `write_document` | Modify that document (return `ContentUpdate`) |
| `read_all_documents` | Search, list and read across the corpus |
| `write_all_documents` | Create documents |
| `read_filesystem` | Read files from disk |
| `write_filesystem` | Write files to disk |
| `network` | Make network requests — no core skill uses it |
| `llm_inference` | Run inference on the local model — core skills only |

---

## Third-party skills — WASM sandbox

**Status:** ◐ Partial — the runtime is built and tested behind the `wasm-plugins` cargo
feature; **no shipped build enables it and no host loads WASM skills**. Hardening items must be
closed before it is wired (private backlog) (code: `crates/sovereign-skills/src/wasm/`,
`crates/sovereign-skills/wit/skill.wit`).

Third-party skills are **WebAssembly components** (Component Model, run by wasmtime).

### WIT world

The contract is `sovereign:skill@0.1.0`, world `skill-plugin`
(`crates/sovereign-skills/wit/skill.wit`):

- **Exports (the skill implements):** `name`, `required-capabilities`, `actions`,
  `file-types`, `execute(action, doc, params, granted-capabilities) -> result<skill-output,
  string>`.
- **Imports (the host provides), interface `host-db`:** `search-documents`, `get-document`,
  `list-documents`, `create-document`.
- **Document across the boundary:** the invocation document is passed as id, title and **body
  text only** — images and videos are not sent. A document fetched through `get-document` is
  returned with its content as stored, not reduced to body text.
- **Output:** `content-update(string)`, `file(file-output)`, `none`,
  `structured-data(structured-output)`. `content-update` carries body text only: the host
  saves it as the document's whole content, so the document's images and videos are dropped.
  **Status:** ◐ Partial — tracked in the backlog (keep attachments on a body-only update).
- **Capability enum (WIT):** `read-document`, `write-document`, `read-all-documents`,
  `write-all-documents`, `read-filesystem`, `write-filesystem`, `network`. There is no
  `llm-inference`.

### Isolation and limits

- No WASI: a guest has no filesystem, network, clock or process access of its own. Its only
  window on the system is `host-db`. The `read_filesystem`, `write_filesystem` and `network`
  capabilities are reserved and grant nothing to a WASM skill.
- **Every host call is capability-checked**, and single-document grants are **scoped**:
  `write_document` may create only in the current thread; corpus-wide reads need
  `read_all_documents`.
- **Fresh `Store` per execution** — skills are stateless.
- **Limits** (defaults): **16 MB** memory, **~10⁹ instructions** of fuel per execution,
  **10** instances. Exceeding a limit terminates the execution with an error to the user.
- A WASM skill registers through the same `CoreSkill` interface as core skills.

**Requirement:** the capabilities granted to a WASM skill are the ones the user approved from
its manifest. **Status:** ○ Specified, not built — to close before the feature is wired.

### LLM inference is not exposed to WASM

**Status:** ✅ Built — `llm-inference` is absent from the WIT capability enum, by design (skills roadmap, Phase 2b).

`llm_inference` stays core-only. Exposing it across the component boundary is its own work:
streaming responses across the boundary, the interaction of fuel limits with multi-second
inference calls, and model selection from inside the sandbox. Deferred until a WASM skill needs
it.

### Discovery

**Status:** ○ Specified, not built in any shipped host.

`SkillRegistry::load_wasm_skills` scans `skills/` for subdirectories holding a `skill.json`
and a `.wasm` component and registers what loads. The manifest is only checked for existence:
name, capabilities, actions and file types are read from the component itself. No host calls
it today. The "Skill Discovery & Loading" section of the author guide describes this intended
flow.

*Replaces v1.x (all ✗ Retired — replaced by the WASM Component Model sandbox):*
- **Tiered native execution** with Landlock (filesystem), seccomp-bpf (syscalls), network
  namespaces and cgroups v2 (resource limits) for community and sideloaded skills —
  replaced by in-process WASM isolation and fuel/memory/instance limits, which are
  cross-platform.
- **IPC over Unix sockets with per-launch session tokens** and the JSON request/response
  format — replaced by typed WIT imports and exports.
- **The Python `sovereign_sdk.GraphDB` SDK** (read, traverse, modify, commit, embeddings) —
  replaced by `SkillDbAccess` for core skills and `host-db` for WASM skills.
- The trust tier `sideloaded` — the built `skill_type` has `core` and `community` only.

---

## Skill manifest

**Status:** ◐ Partial — the format is built and parsed; hosts take core-skill metadata from the
trait, and the manifest is parsed only by `scan_directory`, which no host calls (code:
`crates/sovereign-skills/src/manifest.rs`, `registry.rs`).

**File:** `skill.json`.

```json
{
  "name": "Word Count (WASM)",
  "version": "0.1.0",
  "description": "Word count skill running in a WASM sandbox",
  "author": "Sovereign GE",
  "skill_type": "core | community",
  "capabilities": ["read_document"],
  "file_types": ["md", "txt"]
}
```

All fields are required. Every core skill except text-editor, and every seed (below), has one under `skills/`.
Nothing checks a core skill's manifest against its trait, and some have drifted: the image,
video, pdf-export and file-import manifests list file types that differ from the code's, and pdf-export's
says `community`. The trait holds.

*Replaces v1.x:* reverse-DNS `skill_id`, `trust_tier`, `operates_on`, `mime_types`,
`requires_gpu`, `can_spawn_processes`, `filesystem_paths`, `entry_points`, system and skill
`dependencies`, `author.pgp_key` and `audit_trail`, `resource_limits` — not built. Resource
limits are enforced by the sandbox instead of declared.

---

## Community contribution — spec-as-seeds

**Status:** ✅ Built — 20 seeds in `skills/`.

Answers v1.3 Open Question 8 (community contribution model) for skills. For each skill the
project wants but has not built, `skills/<name>/` holds:

- `skill.json` — proposed name, version `0.0.1`, `skill_type: "community"`, capabilities,
  file types;
- `SPEC.md` — purpose, capabilities, actions, a sample input → output, a complexity estimate,
  the host-db extensions it would need, implementation notes.

No implementation: the point is a complete contract waiting for a contributor. The seeds:
changelog-generator, duplicate-finder, entropy-scanner, epub-export, footnote-collector,
frontmatter-parser, grammar-check, latex-export, lorem-ipsum, meeting-notes-template,
merge-documents, remove-duplicates, slides-export, slug-generator, spell-check,
strip-comments, tag-extractor, template-stamp, word-frequency, wrap-unwrap-lines.

`skills/word-count-wasm/` is the reference WASM skill (built with `build-wasm-skills.sh`).

---

## Not built

**Status:** ○ Specified, not built — kept as intended design (v1.x).

- **Registry and distribution** — `sovereign skill install`, a registry with versioned
  packages, compatibility checks, version pinning per thread.
- **Signing and audit trail** — maintainer signatures and auditor sign-off before a skill can
  be installed.
- **Composition** — the orchestrator plans and runs multi-skill pipelines (orchestrator-mediated
  first, direct skill-to-skill later), shown to the user as a plan (ch. 08, *Learned workflows
  and plan visibility*).
- **Invocation audit log** — every skill run logged (skill, action, documents touched, network
  calls, duration).
- **Provenance tracking** — documents modified by a skill record which skill and version, and
  whether the user reviewed the change.
- **Deprecation policy** — on a vulnerability found after audit: alert and warn, leave enabled
  by default (user sovereignty); auto-disable on a critical vulnerability, explain, offer revert
  or uninstall.
