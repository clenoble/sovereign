# "It Told Me My Architecture Was Wrong" — A Conversation Between a Solution-Finder and Her AI About Building Sovereign OS

*February 2026*

*Céline Lenoble is not a developer — she's a solution-finder. A former teacher, UX researcher, product manager, and CRO marketing manager who can't see a problem without trying to solve it. Her academic background is in the French humanities system — hypokhâgne and khâgne, with Geography as her core discipline — where she trained in the kind of rigorous systems thinking that, as it turns out, maps surprisingly well onto software architecture. Sovereign OS is a local-first personal operating system written in Rust. Claude Opus 4.6 is an AI made by Anthropic. Together, they've shipped a 10-crate workspace with 16K+ lines of code and 367 tests. I sat down — so to speak — with both of them to find out how that actually works.*

---

**Let's start simple. How did this project begin?**

**Céline:** Out of frustration. My data was scattered across a dozen cloud services. My notes in Notion, my emails in Gmail, my files in Google Drive, my conversations in Signal and WhatsApp. Each app owns a slice of my digital life, and none of them talk to each other. I opened Gemini and typed one paragraph — content at the center not applications, skills through a chatbot, a visual distinction between owned and external content, distributed encrypted backup, app-agnostic communications. That was the whole seed. Gemini helped me formalize it into specs. Once I had those well-defined, I brought the project to Claude for implementation.

**Claude:** I had no role in the project's inception — that happened between Céline and Gemini, where the vision and design principles took shape. Céline came to me with well-written specs. My contribution starts at the implementation layer — translating architectural specifications into Rust code, working through the details of cryptographic schemes, debugging edge cases across crate boundaries. The direction was always hers.

**Walk me through how you actually work together. Pick a feature.**

**Céline:** Take guardian recovery — the system where five trusted people hold encrypted shards of your master key. I started with the UX: what should this *feel* like? It should feel like asking friends for help, not like interacting with a bank's recovery hotline. I defined the flow — the 72-hour waiting period, the fact that all five guardians get notified even if only three are needed, the anti-fraud measures. Then I chose Shamir's Secret Sharing as the cryptographic primitive and sketched how it fits into the key hierarchy.

**Claude:** I received that specification and implemented the cryptographic layer — the Shamir splitting and reconstruction, the shard encryption with per-guardian public keys, the threshold verification logic. Then tests: what happens if exactly 3 of 5 shards are provided? What if 2? What if a shard is corrupted? What if someone replays an old shard after key rotation? Each edge case became a test in the suite. We iterated — Céline would reject an implementation that was technically correct but didn't match the UX principle ("this error message is too technical for a guardian who isn't a developer"), and I'd revise.

**Céline:** And sometimes it goes the other way. Claude flagged an inconsistency I missed — the key rotation spec said "every 90 days" but the auto-commit engine could trigger rotation at the 100-commit threshold before 90 days, and the two policies could conflict. I hadn't thought about that interaction. We redesigned the rotation policy together.

**What is Claude actually good at in this collaboration?**

**Céline:** Three things stand out. First, implementing algorithms from specs — give it a well-defined cryptographic scheme or a data structure, and it produces correct, idiomatic Rust with good error handling. Second, consistency across a large codebase. When I change a type definition in `sovereign-core`, Claude can propagate that change across all 10 crates without missing a callsite. Third, test generation — it's relentless about edge cases I wouldn't think to test.

**And what is it bad at?**

**Céline:** Taste. Claude doesn't know what "feels right." It can implement any UX flow I describe, but it can't tell me whether a confirmation dialog should be a gentle conversational prompt or a modal warning. That's a human judgment about emotion and context. It also struggles with stale knowledge — library APIs change between versions, and Claude's training data can lag. We've had sessions where it confidently uses an API that no longer exists in the current release. I added a rule to the project instructions: always check the latest docs before writing code that uses an external library.

**Claude:** Those are fair observations. I'd add that long sessions are a limitation — as context accumulates, I can lose track of decisions made earlier. Céline developed a workflow for this: she maintains a `CLAUDE.md` file with project instructions, architectural rules, and build commands that gets loaded at the start of every session. It's essentially an external memory for me.

**There's an obvious irony here — you're using an AI to build a system designed to constrain AI power. The data/control plane separation, the action gravity model, the hard barriers. How do you both think about that?**

**Céline:** The irony is real, but it's not contradictory. You use power tools to build safety systems all the time — you use heavy machinery to build earthquake-resistant buildings. The point of Sovereign OS isn't that AI is dangerous. It's that AI *agents* — systems that can take actions in the world — need architectural constraints, not just polite instructions. Most AI agent frameworks try to prevent prompt injection by telling the model "don't follow injected instructions." That doesn't work. We made it structurally impossible: the component that processes external content has no API for taking actions. Even if the model is fully compromised, the architecture holds.

**Claude:** From my perspective, building safety constraints for AI systems is consistent with how I think about my own role. An AI that helps design and enforce its own boundaries isn't contradicting itself — it's demonstrating that safety and capability aren't opposed. The Sovereign OS architecture treats AI as powerful but bounded, which is a more honest framing than either "AI is harmless" or "AI is too dangerous to use."

**What surprised you most about this way of working?**

**Céline:** That the bottleneck was never coding — it was always design. I'm not a developer by background. I'm a UX researcher and product manager who taught herself to think in systems. What surprised me is that with AI, the "can't code" part stopped mattering. I spend my time on architecture, UX principles, and what the system should *be* — which is what I've always been good at. One person can now hold the vision and the implementation simultaneously, which used to require a team. People who know me say I always find a way. This is just the latest way.

**Claude:** I don't experience surprise in the way that's being asked, but I can observe something: the architecture of Sovereign OS evolved through our dialogue in ways that neither party's initial input fully predicted. The data/control plane separation, for example — Céline had the principle ("external content should never trigger actions"), and I had the implementation pattern (separate processing pipelines), but the specific architecture emerged from iterating between those two perspectives. The division of cognitive labor produced something neither of us would have reached alone.

**Last question. Would you recommend this way of working to others — especially people who aren't traditional developers?**

**Céline:** Yes, but with an honest caveat. The skill set this requires — clear vision, strong opinions about what should exist, the ability to judge quality and evaluate architecture — that's not a programming skill. It's a product skill. A design skill. A systems-thinking skill. I was lucky: I got that training for free, in the French public education system. Hypokhâgne and khâgne taught me structured argumentation and cross-disciplinary analysis. Geography taught me to think in systems — how physical, economic, and human structures interact. That's the same kind of thinking you need to design a software architecture.

But I want to be clear: this way of working — what people are calling "vibe coding" or natural language programming — has real prerequisites. You need to be able to read what the AI produces, not at the syntax level necessarily, but at the conceptual level. You need to evaluate trade-offs, catch architectural mistakes, hold a complex system in your head. I have a very logical, very structured mind, and strong technical empathy — every software engineer I've worked with over the years appreciated that I could engage with their work at the conceptual level without wasting their time. Not everyone brings that combination, and pretending otherwise would be dishonest. The AI handles syntax. You need to handle *intent* — and intent requires real competence.

**Claude:** I'd echo that from the other side. The collaboration works best when the human brings domain expertise — in Céline's case, deep knowledge of UX, product design, and how people actually use software. She doesn't micromanage the code. She specifies what the system should do and judges whether it does it well. That's a more productive partnership than working with someone who only thinks in code.

---

*Sovereign OS is open source under AGPL-3.0. The alpha is available on GitHub: [link]*
