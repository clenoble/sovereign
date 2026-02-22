# One Person + Claude: Building a 16K-Line Rust OS with AI Collaboration

*Céline Lenoble & Claude Opus 4.6 · February 2026*

Sovereign OS is a 10-crate Rust workspace with over 16,000 lines of code and 367 tests. It includes a graph database layer, an AI orchestrator with two local LLMs, end-to-end encryption with hierarchical key derivation, a GPU-accelerated spatial canvas, a modular skill system with kernel-level sandboxing, peer-to-peer device sync, and multi-channel communications. Two years ago, this would have required a team of developers. I built it without being one.

I'm not a developer. I'm a solution-finder — a former teacher, UX researcher, product manager, and CRO marketing manager who can't see a problem without trying to solve it. It started with a single prompt to Gemini:

> I'd like you to help me formalize a vision I have for a user-focused operating system. It should be open-source, local. The content is at the center, not the applications. Skills through a chatbot. A clear visual distinction between what the user owns versus external sources. Distributed encrypted backup. App-agnostic communication attached to the relevant contact.

That paragraph contained every major architectural decision. Gemini helped me turn it into detailed specifications. Claude Opus 4.6, Anthropic's AI, turned those specs into Rust. This article is an honest account of what that collaboration actually looks like — what works, what doesn't, and what it changes about who gets to build software.

## What the human does

I do the thinking. This sounds reductive, but it's the most accurate description.

I decide what gets built and why. I defined the content-centric paradigm: data as the primary citizen, skills as composable tools, applications as a concept we leave behind. I wrote the UX principles document — eight principles that govern every interaction between the AI and the user, from action gravity (friction scales with irreversibility) to injection surfacing (detected attacks shown, not hidden). I chose the architectural patterns: the data/control plane separation for prompt injection defense, Shamir's Secret Sharing for guardian recovery, SurrealDB for graph storage, Iced for the GUI, Skia for canvas rendering.

This is where my background matters — and not just the professional experience. I went through hypokhâgne and khâgne, the French preparatory classes for the humanities grandes écoles. Free, public, and brutally rigorous. My major, if you can call it that, was Geography — which in the French system is not about memorizing capitals. It's systems thinking: how physical, economic, political, and human systems interact at scale. That training, combined with years of UX research and product management, gave me the mental models to design a complex software architecture without ever having written production code. I also have what I'd call strong technical empathy — I can read Rust, follow the logic across modules, understand trait bounds and lifetime annotations, even if I couldn't write them from scratch. Most software engineers I've worked with over the years liked collaborating with me precisely because of this: I speak their language at the conceptual level, I understand trade-offs, and I don't waste their time. What I lacked was the implementation muscle.

I also make taste decisions. When Claude produces an implementation that is technically correct but doesn't feel right — a confirmation dialog that's too formal, an error message that's too technical for its audience, a data structure that's elegant but doesn't match the user's mental model — I reject it and explain why. Claude can implement any UX flow I describe, but it can't tell me which flow to describe. That judgment — the sense of what "feels right" — remains entirely human.

## What Claude does

Claude implements. Given a well-defined specification — an algorithm, a data structure, an API surface, a set of behaviors with examples — Claude produces correct, idiomatic Rust code with error handling, documentation, and tests. It does this faster than I could, but speed isn't the main advantage. The main advantage is consistency.

A 10-crate workspace has a lot of internal surface area. Types defined in `sovereign-core` are used in `sovereign-db`, `sovereign-ai`, `sovereign-ui`, and `sovereign-app`. When I change a type definition — adding a field, renaming a variant, modifying a trait bound — Claude propagates that change across every crate that uses it. It doesn't miss callsites. It doesn't forget to update the tests. It doesn't lose track of which module depends on which other module. For a single developer, this kind of codebase-wide consistency is the difference between a project that stays maintainable and one that slowly becomes unknowable.

Claude is also relentless about edge cases. When implementing Shamir's Secret Sharing, it didn't just test the happy path (split key, recombine with threshold). It tested: exactly threshold shards, one fewer than threshold, corrupted shards, duplicate shards, shards from different splitting operations mixed together, recovery after key rotation. Each edge case became a test in the suite. Some of these I would have thought of eventually. Others I wouldn't have.

## What doesn't work

Stale knowledge. This is the most persistent problem. Library APIs change between versions, and Claude's training data has a cutoff. We've had sessions where Claude confidently writes code using a method that was renamed or removed in the latest release of a crate. The code compiles in Claude's mental model but fails on the actual toolchain. I added a rule to the project instructions: before writing code that uses an external library, check the latest documentation. Don't rely on memorized APIs. This helps, but doesn't eliminate the problem entirely.

Context loss over long sessions. As our conversation accumulates tokens, Claude can lose track of decisions made earlier. The architecture we agreed on in the first hour might be partially forgotten by the fifth hour. I developed a workflow for this: a `CLAUDE.md` file that contains project instructions, architectural rules, build commands, and key design decisions. It's loaded at the start of every session. It's essentially an external memory. Without it, long sessions drift.

Taste, as mentioned. Claude can generate four alternative implementations of a confirmation dialog and explain the trade-offs of each. What it can't do is feel which one is right for this context, this user, this moment in the workflow. Design is not optimization. It's judgment under ambiguity, and that remains a human capability.

## What changes about who gets to build

The most significant shift isn't about how developers work. It's about who counts as a builder.

I've never identified as a developer. I'm a teacher who became a UX researcher who became a product manager who became whatever this is. What all those roles share is the same instinct: see a problem, find a solution, make it real. The gap was always the last step — making it real at the systems level required skills I didn't have time to spend years acquiring.

AI closed that gap. Not by dumbing things down, but by letting me operate at the level where I'm strongest: vision, architecture, user experience, product judgment. Claude handles Rust's lifetime system and crate dependency graphs. I handle the question of what the system should be and whether it's getting there.

This isn't "no-code." The codebase is 16,000 lines of real Rust. It's that the human contribution shifts from syntax to intent — from "how do I implement this" to "what exactly should this do and for whom." That's a product skill. A design skill. A skill that comes from years of watching people struggle with bad software and knowing what better looks like.

But I want to be honest about something. This way of working has prerequisites. I didn't teach myself systems thinking — I got it from a rigorous public education that most people in the world don't have access to. The preparatory classes and Geography training I described above taught me structured argumentation, cross-disciplinary analysis, and how to think in systems. Product management taught me to decompose complex problems into components. UX research taught me to think from the user's perspective. And I have a particular kind of mind — very logical, very structured — that happens to map well onto software architecture.

"Vibe coding" or natural language programming, as people are starting to call it, will not work for absolutely everyone. If you don't have a clear vision, the AI will happily build the wrong thing with beautiful code and full test coverage. If you can't evaluate what the AI produces — not at the syntax level, but at the architectural level — you won't catch the mistakes that matter. The human's conviction is the guardrail, and conviction without competence is just enthusiasm.

## The irony

There is an irony that I find productive rather than troubling: I'm using an AI to build a system whose core security principle is that AI should never have unchecked power. The data/control plane separation, the action gravity model, the hard barriers enforced by code rather than prompts — these are all constraints on AI agency. And yet the system itself was built by an AI.

I don't see a contradiction. Building safety constraints for powerful tools is normal. Engineers who design airbags drive cars. Architects who design fire exits work in buildings. The people best positioned to build constraints for AI are those who work with AI closely enough to understand both its capabilities and its failure modes. Claude helped implement the injection surfacing system — the code that detects and displays prompt injection attempts to the user. In doing so, it contributed to a system that constrains models like itself. I think that's healthy.

## Practical advice

For anyone considering this way of working — especially non-developers with domain expertise and strong opinions — the things that matter most in my experience are: maintain a project instructions file that captures architectural decisions (Claude can't remember what you agreed on last Tuesday), verify library versions before letting the AI write code that depends on them, and most importantly, know what you want to build before you ask the AI to build it.

The last point is the most important. AI collaboration amplifies whatever you bring to it. If you bring clarity, you get implementation. If you bring ambiguity, you get plausible-sounding code that may or may not solve your actual problem. The human's job is to eliminate ambiguity before it reaches the AI. This is a form of discipline that looks nothing like writing code, but it's where most of the value comes from.

I feel genuinely privileged to be able to build something like this. Not because I'm exceptional, but because I had access to things that made it possible: a free, rigorous education that trained me to think structurally; a career path that happened to cross UX, product, and systems design; a logical mind that takes naturally to architecture; and now, an AI capable enough to turn well-specified intent into working code. Not everyone has that combination, and pretending otherwise would be dishonest. People who know me say I always find a way. AI didn't change that instinct. It just removed the last barrier between the solution I could see and the system I could ship.

*Sovereign OS is open source under AGPL-3.0. GitHub: [link]*
