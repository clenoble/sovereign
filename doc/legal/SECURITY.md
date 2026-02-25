# Security Policy

## Sovereign GE — Security Disclosure Policy

---

## Supported Versions

| Version | Supported |
|---|---|
| development (main branch) | ✅ Active |
| No stable release yet | — |

This policy will be updated when stable releases begin. Only the latest stable release and the current development branch will receive security patches.

---

## Reporting a Vulnerability

**Do NOT open a public GitHub issue for security vulnerabilities.**

### How to Report

1. **Email:** Send a report to **[SECURITY_EMAIL]**
2. **Encryption:** Use our PGP key to encrypt your report (key available at `[KEY_URL]`)
3. **GitHub Security Advisories:** Alternatively, use GitHub's private vulnerability reporting feature on the repository

### What to Include

- Description of the vulnerability
- Steps to reproduce (minimal, specific)
- Affected component(s) (`sovereign-db`, `sovereign-ai`, `sovereign-skills`, etc.)
- Impact assessment (what an attacker could achieve)
- Any suggested fix, if you have one

### What to Expect

| Step | Timeline |
|---|---|
| Acknowledgment of receipt | Within 48 hours |
| Initial triage and severity assessment | Within 7 days |
| Fix developed and tested | Depends on severity (see below) |
| Public disclosure | Coordinated with reporter |

### Severity Response Targets

| Severity | Description | Target Fix Time |
|---|---|---|
| **Critical** | Remote code execution, key material exposure, data exfiltration | 72 hours |
| **High** | Privilege escalation, encryption bypass, skill sandbox escape | 7 days |
| **Medium** | Information disclosure, denial of service, injection surfacing bypass | 30 days |
| **Low** | UI spoofing, minor information leak, non-exploitable weakness | Next release cycle |

---

## Scope

### In Scope

- **sovereign-core:** Runtime, lifecycle, configuration
- **sovereign-db:** SurrealDB abstraction, document storage, encryption at rest
- **sovereign-skills:** Skill registry, IPC protocol, execution environment, resource limits
- **sovereign-canvas:** Rendering, input handling
- **sovereign-ui:** GTK4 shell, search, document windows
- **sovereign-ai:** PyO3 bridge, model loading/unloading, memory management
- **Python AI layer:** Intent classification, voice pipeline, user profile
- **P2P sync protocol:** Device-to-device sync, manifest exchange, conflict resolution
- **Guardian protocol:** Shard generation, distribution, recovery flow
- **Identity Firewall:** Cookie isolation, fingerprint randomization, Trusted Domain List
- **Encryption:** Key hierarchy, key derivation, at-rest and in-transit encryption
- **Audit log:** Integrity (append-only, hash chain), access controls

### Out of Scope

- Third-party AI model weights (report to upstream: Hugging Face, Meta, Microsoft, etc.)
- Third-party system dependencies (GTK, Skia, GStreamer — report to upstream projects)
- NVIDIA CUDA vulnerabilities (report to NVIDIA)
- Social engineering attacks that require physical access to an unlocked device
- Vulnerabilities in user-installed community skills (report to skill maintainer, but we welcome reports about the skill sandboxing system itself)

---

## Security Architecture Overview

For reporters unfamiliar with the codebase, here is a summary of the security-relevant architecture:

### Key Hierarchy

```
Master Key (256-bit, TPM/secure enclave)
  ├─> Device Keys (HKDF per device)
  │     └─> Document Keys (HKDF per document)
  └─> Recovery Key (Shamir 3-of-5 split → Guardian shards)
```

### Hard Barriers (Enforcement Layer, Not Model Layer)

These constraints are enforced by code, not by AI model instructions. A vulnerability in any of these is considered **High** or **Critical** severity:

| Constraint | Enforcement |
|---|---|
| External content cannot invoke skills | Data plane has no skill API path |
| Network-accessing actions require user approval | Skill execution layer checks action level |
| Document deletion has 30-day undo | DB layer (soft delete only) |
| Audit log is append-only | Filesystem immutable append flag + hash chain |
| Level 3+ actions require explicit user confirmation | Action dispatcher checks level pre-execution |

### Prompt Injection Boundary

The AI orchestrator processes external (untrusted) content. The security model assumes the model can be fully compromised by adversarial input. All safety-critical invariants are enforced at the execution layer, not the model layer. A vulnerability that allows external content to trigger actions *without* passing through the action level check is **Critical**.

---

## Disclosure Policy

We follow **coordinated disclosure**:

1. Reporter sends vulnerability details privately.
2. We acknowledge, triage, and develop a fix.
3. We coordinate a disclosure date with the reporter (default: 90 days from report, or when the fix is released, whichever comes first).
4. We publish a security advisory on GitHub with credit to the reporter (unless they request anonymity).
5. We release the patched version.

We will **never** take legal action against security researchers acting in good faith under this policy.

### Safe Harbor

Activities conducted consistent with this policy are considered authorized. We will not pursue civil or criminal action against researchers who:

- Act in good faith to avoid privacy violations, data destruction, and service disruption
- Only interact with accounts they own or with explicit permission
- Report vulnerabilities promptly and do not disclose publicly before coordinated disclosure
- Do not exploit vulnerabilities beyond what is necessary to demonstrate the issue

---

## Threat Model

Sovereign GE considers the following threat actors in its security model:

| Threat Actor | Assumed Capability | Primary Defense |
|---|---|---|
| **Malicious website** | Arbitrary content, prompt injection | Data/control plane separation, hard barriers |
| **Compromised community skill** | Code execution within skill sandbox | Landlock LSM, resource limits, audit logging |
| **Stolen device** | Physical access to powered-off device | Full-disk + per-document encryption, TPM-bound keys |
| **Malicious Guardian (1-2 of 5)** | Holds shard(s), may collude | Shamir 3-of-5 threshold; 1-2 shards insufficient |
| **Network attacker** | MITM, traffic analysis | TLS 1.3 + per-document encryption for P2P sync |
| **Compromised AI model** | Arbitrary output from model | Execution layer enforces all invariants; model is untrusted |

---

## Bug Bounty

There is currently no paid bug bounty program. We will credit reporters in security advisories and the project changelog. If a bounty program is established in the future, this document will be updated.

---

*This policy is based on the [GitHub Security Advisories](https://docs.github.com/en/code-security/security-advisories) framework and the [disclose.io](https://disclose.io/) Safe Harbor terms.*
