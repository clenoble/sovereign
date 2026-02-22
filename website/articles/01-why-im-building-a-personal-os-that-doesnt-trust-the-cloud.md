# Why I'm Building a Personal OS That Doesn't Trust the Cloud

*Céline Lenoble & Claude Opus 4.6 · February 2026*

The cloud is someone else's computer. This is not a metaphor. Your notes in Notion sit on AWS servers in Virginia. Your emails in Gmail pass through Google's infrastructure in Oregon, Belgium, Taiwan. Your conversations in WhatsApp are routed through Meta's data centers. Every document you create, every message you send, every thought you commit to digital form — it all lives on hardware you don't control, governed by terms of service you didn't read, subject to policies that can change without notice.

This arrangement has become so normal that questioning it sounds paranoid. But consider what it means in practice. Google can disable your account and lock you out of a decade of email, documents, and photos — and they do, regularly, with no human review and no appeal. Apple can reject an app update and kill a business overnight. Notion can change their pricing, their API, their data format, and your only option is to comply or migrate. The pattern is always the same: convenience in exchange for control.

I don't think this trade-off is necessary. I think it persists because we haven't built the alternative.

## The application-centric trap

The deeper problem isn't cloud storage per se — it's the application-centric model that cloud services are built on. In this model, your data doesn't really belong to you. It belongs to the application. Your notes are "Notion notes," not notes that happen to be edited in Notion. Your spreadsheets are "Google Sheets," not spreadsheets that happen to live on Google's servers. The data format, the access method, the storage location, the sharing model — all of it is controlled by the application vendor.

This creates a form of lock-in that goes beyond economics. It fragments your digital life into silos that don't communicate. Your research notes are in one app, your project plans in another, your emails in a third, your files in a fourth. There is no unified view of your own work. There is no way to draw a line between a note you wrote in March and the email that inspired it, unless both happen to live in the same application — which they almost never do.

What if we inverted this? What if data were the primary citizen, and applications were just tools that operate on it?

## Content-centric instead of application-centric

This is the premise behind Sovereign OS. Instead of applications that own your data, you have a local graph database that stores everything: documents, threads, relationships between documents, version history, contacts, conversations. Skills — lightweight, composable tools — operate on this data. A markdown editor skill, a PDF export skill, an image viewer skill. They can read and modify your documents, but they don't own them. You can switch skills freely, without migration, without data loss, without asking anyone's permission.

The graph structure matters. A document can be linked to any number of other documents through typed relationships: references, derives-from, continues, contradicts, supports. These relationships carry metadata — strength, direction, the state of the target document at link time. Your knowledge isn't trapped in folders. It's a living network that reflects how ideas actually connect.

And all of it lives on your machine, encrypted with keys you control.

## The AI question

Then there's the AI problem — or rather, the AI opportunity, depending on how you build it.

Every major AI assistant today sends your data to external servers. ChatGPT, Copilot, Gemini — they all require your prompts, your documents, your questions to leave your machine and travel to a data center where inference happens on someone else's GPU. For many use cases this is fine. For a personal operating system that handles your most private documents, communications, and thoughts, it's unacceptable.

Sovereign OS runs AI locally. Two Qwen2.5 models via llama.cpp: a small 3B router that's always loaded for fast intent classification, and a larger 7B reasoning model that loads on demand for complex queries. Voice input through Whisper, text-to-speech through Piper, wake-word detection through Rustpotter. All of it on-device. Your prompts never leave your machine. There's no API key, no subscription, no telemetry.

This is possible now because quantized models have become good enough to run on consumer hardware. A mid-range gaming PC — six-core CPU, 16 GB RAM, an RTX 3060 — can run the entire stack. Two years ago this would have been impractical. Today it works.

But running AI locally isn't just about privacy. It's about trust in a more fundamental sense. When the AI that helps you organize your digital life runs on external servers, you're trusting the server operator not just with your data, but with the AI's behavior. You're trusting that the model hasn't been fine-tuned to subtly prefer certain outcomes, that the system prompt hasn't been modified to serve the operator's interests, that the inference pipeline hasn't been altered to extract information from your queries. Running locally eliminates this entire class of concerns. The model is a file on your disk. You can inspect it, replace it, verify it.

## What about when things go wrong?

The standard objection to local-first systems is recovery. What happens when your hard drive fails? What happens when your laptop is stolen? With cloud services, recovery is trivial — log in from another device and everything is there. With local-first, if you lose the device, you lose the data.

Sovereign OS addresses this through two mechanisms. First, peer-to-peer sync across your own devices using libp2p. Your phone, your laptop, your desktop — they sync directly, encrypted, without any server in the middle. This handles the common case of a single device failure.

Second, for catastrophic loss — all devices destroyed — there's guardian social recovery. You choose five trusted people. Your master recovery key is split into five encrypted shards using Shamir's Secret Sharing. Any three of five can reconstruct the key. But there's a 72-hour waiting period during which all five guardians are notified, so if someone is trying to impersonate you, the real you can abort the recovery. Guardians never see your data. They hold opaque encrypted blobs. They don't need to understand cryptography. They just need to be people you trust.

This replaces corporate account recovery with a human trust network. Instead of proving your identity to a corporation through a selfie and an uploaded passport scan, you prove it to people who know you. This feels more honest to me. It reflects how trust actually works in life — through relationships, not through bureaucratic verification.

## The bet

I'm not a developer. I'm a solution-finder — a former teacher, UX researcher, product manager, and CRO marketing manager who can't see a problem without trying to solve it. My training is in the humanities: hypokhâgne and khâgne in the French public system, with Geography as my discipline — which, in France, is fundamentally about understanding how complex systems interact. That education, free and rigorous, gave me the analytical tools to design software architecture. The career that followed — teaching, then UX research, then product management — gave me the user-centered instincts. I can read code, follow logic across modules, evaluate architectural trade-offs. I just don't write it. The very first prompt I sent to Gemini, before any code existed, was this:

> I'd like you to help me formalize a vision I have for a user-focused operating system. It should be open-source, local. The content is at the center, not the applications. That means that a user can view any content directly in the OS, and call on "skills" through a chatbot to edit the document in any way they need. There is still a "browser" for internet content, or at least a clear visual distinction between what the user owns versus external sources. Cloud backup should be distributed, not centralized, and encrypted. Communication is app-agnostic, and attached to the relevant contact. Centralized management of PII and cookies.

Every major feature of Sovereign OS — content-centric architecture, skills instead of apps, the owned-versus-external visual language, distributed encrypted sync, unified communications, privacy management — traces back to that single paragraph. Gemini helped me formalize these ideas into specifications. The Rust implementation comes from Claude Opus 4.6, Anthropic's AI. I provide the vision, the UX principles, the architectural decisions; Claude provides the code across a 10-crate Rust workspace. This is itself an experiment in what one person can build when augmented by AI — not replacing the human, but proving that the bottleneck was never coding. It was always knowing what to build.

There's an irony here that I don't shy away from: an AI helping build a system whose core principle is that AI should never have unchecked power. But this isn't contradictory. You use power tools to build safety systems. The point isn't that AI is dangerous. The point is that AI agents — systems that can take actions in the world — need architectural constraints, not just polite instructions. More on that in a separate article.

Sovereign OS is in alpha. It's rough around the edges. It runs on Linux, Windows, and macOS. It's open source under AGPL-3.0. I'm not claiming it replaces everything today. I'm claiming that the model it represents — local-first, encrypted, content-centric, AI-augmented — is a better model than the one we've accepted as default.

What we build is what will be. I'd rather build something I believe in.

*Sovereign OS is open source under AGPL-3.0. GitHub: [link]*
