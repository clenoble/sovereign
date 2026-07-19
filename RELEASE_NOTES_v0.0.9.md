# Sovereign GE v0.0.9

*Your data, your rules.* The v0.1 bar is **data you can trust + daily driver on
desktop**. v0.0.9 is a large step toward it: bring your real documents in, harden
social recovery, and a broad security pass.

## Highlights

### Bulk import — bring your corpus in
A stub-first import funnel lands your whole document tree on day one. Folders
become lanes and original file timestamps are preserved. Text documents (Markdown,
plain text, CSV and similar) import as editable content; common rich formats
(HTML, PDF, Word, PowerPoint) are extracted to editable text; everything else —
including source code and config, which are deliberately never imported — lands as
a findable, re-processable stub, so nothing in the tree is silently dropped. A
dry-run shows the full plan before anything is written.

Imported files become **your own** (control-plane) content — and the importer
says so plainly, including what that means: owned content is trusted and not
screened for prompt-injection, so you decide what enters your workspace.

### Social recovery with guardians (new)
v0.0.9 introduces **guardian-based recovery** — a whole new way back into your
workspace if you forget your passphrase, with no company or central service that
could lock you out or be compelled to let someone else in.

- **Enroll guardians.** You designate trusted people as your guardians. Each one's
  device holds a single encrypted shard of your recovery key — no guardian can do
  anything with their shard alone. A companion guardian app lets them hold their
  shard and approve a recovery request.
- **Recover with a threshold.** If you lose your passphrase, a threshold of your
  guardians (3 of 5) together restore your access — never fewer, never a single
  party.
- **Protected end to end.** The shards collected during a recovery are sealed at
  rest under the new passphrase you set at the very start of the wizard, so
  recovery material is never left unprotected while the process is under way.

### Security hardening — a broad review and its fixes
- **Stronger prompt-injection defenses.** Untrusted content — documents, saved web
  pages, synced data — is more robustly fenced from the on-device AI across all
  supported models. Detected injection attempts are now **surfaced** to you rather
  than handled silently, and in the AI agent loop **you choose** how a flagged
  item is handled.
- **Safer document import.** Office documents are bounded against decompression
  ("zip-bomb") abuse, and imported file timestamps can't distort your timeline.
- **Hardened relay / seed node.** The public store-and-forward node is protected
  against unauthenticated resource-exhaustion and message-flooding, with a
  capability mechanism ready for when store-and-forward is switched on.
- **Native-shell at-rest parity.** The default desktop shell now installs the same
  encrypted session log and PII protections the Tauri build already had.

### Native desktop shell
Continued build-out of the native Rust shell (Vello/winit/wgpu) as the default
desktop UI, including the access-recovery wizard surfaces.

## Notes
- Desktop is the daily-driver target; the mobile Tauri app continues alongside.
- Local-first, on-device AI, field-level encryption, peer-to-peer sync — unchanged
  foundations.
- Fully open source (AGPL-3.0).
