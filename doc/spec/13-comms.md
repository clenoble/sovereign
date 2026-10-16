# 13 — Communications

Email, Signal and WhatsApp in one place, organized around people rather than apps, stored
encrypted like every other piece of the user's data. The engine exists; no shipped build
connects to a mail server or a messaging network yet.

*Consolidates:* specification v1.3 Appendix A ("Unified Comms ✅ Implemented" — corrected
below) and the comms mentions in v1.3 §Relationship Model; archived plans `v0.0.9-plan.md`
(email deferral) and `v0.0.8-release-plan.md` (COMMS-001); code in `crates/sovereign-comms`,
`crates/sovereign-shell/src/comms.rs`, the comms commands and PII hooks in
`crates/sovereign-app`, and the message/contact storage in `crates/sovereign-db`.

**Status:** ◐ Partial — **dormant**. The channel engine, the contact-centric inbox and
encrypted storage are built; no shipped build constructs a channel. Email is deferred to
a later release (not yet numbered) together with its hardening; Signal follows (ch. 15, *Roadmap*).

*Replaces v1.x:* Appendix A listed Unified Comms as "✅ Implemented — Email, Signal, WhatsApp
channels with sync engine". The engine was implemented and unit-tested, but no running
application used it until email briefly went live in the native shell during the v0.0.9 cycle;
it was turned off again before the release, unhardened (see *Email*).

---

## Channel model

**Status:** ✅ Built — engine only (code: `crates/sovereign-comms/src/channel.rs`,
`sync_engine.rs`, `config.rs`).

Every channel implements one trait, `CommunicationChannel`:

| Method | Purpose |
|---|---|
| `connect` / `disconnect` / `status` | Session with the remote service |
| `fetch_messages(since)` | New messages since a timestamp |
| `send_message(OutgoingMessage)` | Send; returns the external message id |
| `sync` | One full cycle: fetch, persist, update conversations |
| `resolve_contact(address)` | Map an address to a contact, creating a stub contact if new |

- `CommsSync` polls the registered channels (default every **300 s**) and emits sync-complete
  and sync-error events. The new-messages and contact-discovered events are defined but never
  emitted, and every channel reports zero new contacts.
- Configuration lives in `comms.toml` in the Sovereign directory: host, port, username,
  display name, phone number — **never a password or token**.
- Each channel is a cargo feature of `sovereign-comms`: `email`, `signal`, `whatsapp`.

## Channels

### Email

**Status:** ◐ Partial — IMAP/SMTP engine built and tested; **disabled at runtime since v0.0.9**
(code: `crates/sovereign-comms/src/channels/email.rs`, `crates/sovereign-shell/src/comms.rs`).

- IMAP over TLS (default port 993) fetches by date; SMTP with STARTTLS (default port 587)
  sends; messages are parsed into the common `Message` model.
- **Credential:** the account password is held in the **PII vault** — a `PiiRecord` of kind
  `Password`, encrypted under the AccountKey (ch. 09) — never in `comms.toml`.
- **Dormant.** The native shell compiles the engine behind a runtime flag that is off; with it
  off, no channel is created, no connection is opened, no mail is fetched. The Tauri backend
  can save an email configuration but never creates a channel; its desktop app is frozen and
  not shipped, and the mobile build excludes comms.

**Deferral (decided 2026-07-06; confirmed at the v0.0.9 release gate, 2026-07-19).** Email
wiring was deferred to one coherent comms release (originally v0.0.10; since 2026-10-10 a later, unnumbered release) with the comms findings of the
v0.0.8 review. Email is not the migration channel either: the user's own documents would arrive
through the data plane as external content, and through a cloud detour (migration is
stub-first bulk import, ch. 03). Email nevertheless went live in the shell during the v0.0.9
cycle and was caught at the release gate — the comms findings had been deferred on the premise
that email was dormant. Decision: ship nothing on email, unwire it, keep the deferral.

**Required before email is re-enabled (the comms release), in the native shell** — the only desktop UI
(the Tauri desktop app is frozen; the mobile build excludes comms):

- every synced and sent message passes through the PII pipeline (message, contact and share
  hooks — ch. 09) before it is used anywhere;
- TLS certificate pinning for the configured servers;
- correct handling of raw addresses;
- OAuth2 (XOAUTH2) sign-in for providers that require it;
- a sync cursor persisted per account, so each sync fetches only what is new. **Status:** ○ —
  the shell builds a fresh channel for every sync, so the cursor is never kept and each sync
  starts over from the first messages of the mailbox;
- sent mail stored in its conversation, so the outbound hooks fire and the sharing ledger
  (ch. 09) records it. **Status:** ○ — the shell's compose sends without a conversation, and
  the sent message is not stored.

Deduplication by Message-ID is built: a synced message whose Message-ID is already stored is
skipped (code: `crates/sovereign-comms/src/channels/email.rs`).

### Signal

**Status:** ◐ Partial — built behind the `signal` feature, never enabled in a shipped build
(code: `crates/sovereign-comms/src/channels/signal.rs`).

Connects as a **linked secondary device** (like Signal Desktop) through `presage` 0.7; the
Signal protocol keys live in presage's own store under the Sovereign directory. Excluded from
the mobile build: presage's message stream is not `Send`, which the channel trait requires; a
local-executor wrapper is needed first.

**Required before Signal ships** (after email): the at-rest handling of the Signal protocol
store is reviewed against the field-encryption model (ch. 07).

### WhatsApp

**Status:** ◐ Partial — types, send path and webhook payload parsing are built behind the
`whatsapp` feature; there is no webhook receiver and no shipped build enables it (code:
`crates/sovereign-comms/src/channels/whatsapp.rs`).

Uses Meta's **WhatsApp Cloud API** (requires a WhatsApp Business account and access token).
The Cloud API is push-based: messages would arrive at a webhook, which does not exist; polling
`sync` is a no-op.

**Required before WhatsApp ships:** webhook payloads are authenticated (HMAC signature check)
before they are parsed.

---

## Contact-centric inbox

**Status:** ◐ Partial — built on desktop (native shell) and mobile (Svelte), over whatever
messages the database holds; marking messages read exists on mobile only (code:
`crates/sovereign-shell/src/panels.rs`; `frontend/src/lib/components/InboxPanel.svelte`,
`ContactPanel.svelte`; `crates/sovereign-app/src/tauri_commands/contacts.rs`).

**The inbox is a list of people, not of channels.** Contacts are sorted by unread count; opening
a contact shows their addresses, one tab per conversation (one per channel), and the unified
message thread. On mobile, opening a contact marks its unread messages read; the shell has no
mark-read (ch. 12). The shell has a compose window wired to the dormant email send path; it does
not store the mail it sends (see *Email*).

Contacts, conversations and messages are graph nodes: a contact can be linked to documents
(`contactof`, ch. 03). The orchestrator reads them through its `search_messages` and
`list_contacts` tools (ch. 08); message content returned to the model is PII-redacted.

## Encryption at rest

**Status:** ✅ Built (code: `crates/sovereign-db/src/encrypted.rs`).

Under the field-encryption decorator (ch. 07), each with its own key database:

| Record | Encrypted fields |
|---|---|
| Message | body, subject, HTML body |
| Conversation | title |
| Contact | name, notes, addresses |

Plaintext values are blanked before the first insert, so no comms content reaches the store in
clear.

## PII integration

**Status:** ◐ Partial — the hooks are defined in `sovereign-comms` and implemented in the
Tauri backend crate (`sovereign-app`) behind the `comms` feature (the message and contact hooks
also need `encryption`), which the native shell does not link; no channel is constructed with
them yet (code: `crates/sovereign-comms/src/pii_hook.rs`, `crates/sovereign-app/src/pii_*_hook.rs`).

Three hooks keep `sovereign-comms` independent of the AI crate:

- `MessageIngestHook` — after a message is persisted, scan and tokenize its body.
- `ContactIngestHook` — on a new contact, record its addresses in the PII inventory (the
  addresses themselves stay as they are: they are the identifier) and tokenize its notes.
- `ShareIngestHook` — after an outbound message, write a share record per PII token and
  recipient, building the sharing ledger (ch. 09). Inbound messages do not fire it: receiving
  personal data is not a disclosure.

Wiring all three on every channel is part of the comms-release requirement above. For the shell to
use them, the implementations have to move out of `sovereign-app` into a crate the shell
links. Their documentation counts on a background PII sweep to catch what a failed hook
missed; nothing schedules that sweep yet (ch. 15, D4).

---

## Not built

**Status:** ○ Specified, not built.

- Comms enabled in any shipped build (the comms release: email; then Signal).
- Pre-send confirmation of personal data leaving through a message (ch. 09).
- Virtual numbers and email forwarding for others (v1.3 §Identity Firewall, future).
