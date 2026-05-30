# Privacy Policy

## Sovereign GE — Privacy Policy

**Effective Date:** [DATE]  
**Last Updated:** February 22, 2026

---

## The Short Version

Sovereign GE is software that runs on your device. We don't collect your data. We can't see your data. We have no servers, no accounts, no analytics, no telemetry. There is nothing to disclose because there is nothing to collect.

The rest of this document explains this in the detail that regulations require.

---

## 1. Who We Are

Sovereign GE is an open-source software project distributed under the GNU Affero General Public License v3.0 (AGPL-3.0). The source code is publicly available at **[REPOSITORY_URL]**.

The project is maintained by **[ENTITY_NAME]** ("we," "us," "our").

**Contact:** [CONTACT_EMAIL]

---

## 2. What This Policy Covers

This policy covers the Sovereign GE software and the project's online presence (website, repository, documentation). It does **not** cover:

- Third-party services you choose to connect to Sovereign GE (cloud backup providers, API endpoints, etc.)
- Community skills developed by third parties
- Websites you visit using Sovereign GE
- Any fork or derivative of Sovereign GE maintained by others

You are responsible for reviewing the privacy practices of any third-party service you use in conjunction with Sovereign GE.

---

## 3. Data We Collect

### 3.1 Through the Software

**None.**

Sovereign GE runs entirely on your device. The software does not:

- Transmit data to us or any third party
- Include telemetry, analytics, crash reporting, or usage tracking
- Phone home, check for updates automatically, or ping any server
- Create or require user accounts
- Contain advertising or ad-tracking code

All data you create, import, or process with Sovereign GE remains on your hardware under your exclusive control. We have no technical means to access it.

### 3.2 Through the Project Website

If we operate a project website ([WEBSITE_URL]), it may use:

- **Server logs:** Standard web server logs (IP address, user agent, requested URL, timestamp). These are retained for a maximum of 30 days for security and abuse prevention, then deleted. No analytics platform processes these logs.
- **No cookies:** The project website does not set cookies, use tracking pixels, or employ fingerprinting.
- **No JavaScript analytics:** No Google Analytics, Plausible, Matomo, or equivalent.

### 3.3 Through the Repository (GitHub)

The source code repository is hosted on GitHub. Your interactions with the repository (issues, pull requests, discussions) are governed by [GitHub's Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement). We do not control GitHub's data practices.

### 3.4 Through the Skill Registry

If the project operates a skill registry (registry.sovereign.org or equivalent), it may log:

- **Download counts** (aggregate, not per-user)
- **IP addresses** in standard server logs (retained max 30 days)

The registry does not require accounts. No personal information is collected or stored to download or install skills.

### 3.5 Through the Trusted Domain List

The Trusted Domain List (TDL) used by the Identity Firewall is a static file distributed with the software. Fetching TDL updates, if implemented, uses a standard HTTPS request. We may log aggregate download counts. We do not log which domains individual users add to or remove from their local TDL.

---

## 4. Data the Software Creates on Your Device

Sovereign GE creates and stores data locally on your device. This data never leaves your device unless you explicitly initiate a transfer (P2P sync, export, Guardian shard distribution). For transparency, here is what the software stores:

| Data | Location | Purpose |
|---|---|---|
| Documents and media | `~/.sovereign/documents/` | Your content |
| Intent thread manifests | `~/.sovereign/threads/` | Project organization |
| Version history | Embedded in document JSON | Undo, branching, audit |
| AI user profile | `~/.sovereign/orchestrator/user_profile.json` | Adaptive learning, preferences |
| Session logs | `~/.sovereign/orchestrator/session_log.jsonl` | Context continuity, search |
| Audit log | `~/.sovereign/audit/` | Security transparency |
| Encryption keys | System secure enclave / TPM | Data protection |
| AI model weights | `~/.sovereign/models/` | Local inference |

**All of this data is yours.** You can read, export, modify, or delete any of it at any time. The software does not transmit any of it without your explicit action.

### Session Logs and User Profile

The AI orchestrator maintains a session log and a user profile to provide contextual, adaptive interaction. These files:

- Are stored in plaintext JSON on your device (encrypted at rest with your device key)
- Are never transmitted to us or any third party
- Can be viewed, exported, and deleted by you at any time
- Have configurable retention (default: 30 days full logs, 1 year compressed summaries, then deleted)
- Can be disabled entirely in Settings

### Voice Data

If you use voice interaction:

- Audio is processed entirely on-device by local models (Whisper, Piper)
- A rolling buffer of the last 10 seconds of audio exists in RAM only and is continuously overwritten
- No audio is written to disk unless you explicitly enable audio logging (off by default)
- No audio is ever transmitted to any external service

---

## 5. Data Sharing

### 5.1 With Us

We receive **no data** from your use of the software. We have no capability to request, access, or compel the transmission of your data.

### 5.2 P2P Sync (Your Devices)

When you enable device-to-device sync, encrypted document fragments are transmitted between your own devices over your local network or the internet. This traffic is encrypted with your device keys. We do not operate relay servers. If a rendezvous server is used for device discovery, it sees only device identifiers and IP addresses, not document content. Rendezvous server logs, if any, are retained for a maximum of 7 days.

### 5.3 Guardian Recovery (Trusted Contacts)

When you configure Guardian recovery, encrypted key shards are distributed to your chosen Guardians. Guardians receive an opaque encrypted blob — they cannot read its contents. The Guardian protocol does not transmit document content, metadata, or any personal information beyond a display name you choose.

### 5.4 Cloud Backup (Optional, User-Initiated)

If you configure optional cloud backup (self-hosted or third-party), data is encrypted with your keys before transmission. We do not operate cloud backup servers. The privacy practices of your chosen backup provider are governed by their terms, not ours.

### 5.5 Skills with Network Access

Some skills may require network access (e.g., a web clipper, an API connector). The skill manifest declares `requires_network: true/false`. Network-accessing actions are Level 3 (Transmit) in the Action Gravity Model and always require your explicit approval before execution. Skill network activity is logged in the audit log.

---

## 6. Third-Party AI Models

Sovereign GE ships with pre-configured AI models that run locally on your device. These models:

- Process data on-device only
- Do not transmit data to model providers (Microsoft, Meta, OpenAI, etc.)
- Do not "call home" or report usage statistics

If you choose to configure an optional cloud AI API (explicitly disabled by default), your data will be transmitted to that provider under their privacy terms. Sovereign GE will display a clear warning before any data is sent to a cloud AI service.

---

## 7. Children's Privacy

Sovereign GE does not collect personal information from anyone, including children. The software has no age-gating because there is no data collection to restrict.

---

## 8. International Data Transfers

Sovereign GE does not transfer your data internationally — or at all. Your data resides on your devices in your jurisdiction.

The project website and repository are hosted on infrastructure that may be located outside your country. Your interactions with those platforms (visiting the website, opening an issue on GitHub) are governed by the respective hosting provider's policies.

---

## 9. Your Rights

Under GDPR, Swiss FADP/nDSG, CCPA, and other data protection regulations, you have rights regarding your personal data. Because we collect no personal data through the software, most of these rights are satisfied by default:

| Right | Status |
|---|---|
| **Access** | We hold no data about you. The software stores data only on your device — you already have full access. |
| **Rectification** | You can modify any data on your device at any time. |
| **Erasure** | You can delete any data on your device at any time. Uninstalling the software removes all local data. |
| **Portability** | All data is stored in open JSON formats. You can export, copy, or migrate it freely. |
| **Restriction of processing** | No processing occurs on our side. |
| **Objection** | There is no processing to object to. |
| **Automated decision-making** | The local AI makes suggestions; you retain full control over all actions. No decisions are made about you by us. |

For data associated with the project website server logs, or your GitHub interactions, contact us at **[CONTACT_EMAIL]** or exercise your rights directly with GitHub.

---

## 10. Data Protection Officer

Given that the project collects no personal data through the software, a Data Protection Officer is not currently appointed. If the project's data practices change (e.g., operating a service with user accounts), this section will be updated.

For privacy inquiries: **[CONTACT_EMAIL]**

---

## 11. Changes to This Policy

If this policy changes, we will:

- Update the "Last Updated" date at the top
- Describe the changes in the repository commit message
- For material changes, publish a notice on the project website and repository

Because the software has no update mechanism that contacts our servers, we cannot push policy changes to existing installations. You can review the current policy at any time in the repository or on the project website.

---

## 12. Regulatory Compliance

### GDPR (EU)

**Legal basis for processing:** Not applicable — no personal data is processed by the project. The software processes data on the user's device under the user's control; the project is neither a data controller nor a data processor with respect to user content.

### Swiss FADP / nDSG

The project does not process personal data of users located in Switzerland. Server logs on the project website, if any, are retained for a maximum of 30 days. No cross-border data transfers occur through the software.

### CCPA (California)

The project does not sell personal information. The project does not collect personal information through the software. The "Do Not Sell My Personal Information" right is satisfied by default.

---

## 13. Contact

For privacy questions, concerns, or requests:

**Email:** [CONTACT_EMAIL]  
**Repository:** [REPOSITORY_URL]  
**PGP Key:** [KEY_URL]

---

*This privacy policy is provided in good faith and describes the project's current data practices. It does not constitute legal advice. The project is distributed software under AGPL-3.0; users are responsible for their own compliance with applicable privacy laws when using the software.*
