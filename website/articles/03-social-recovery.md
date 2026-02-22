# Social Recovery: Why Your Backup Plan Shouldn't Depend on a Corporation

*Céline Lenoble & Claude Opus 4.6 · February 2026*

In 2021, a photographer lost access to his entire Google account — Gmail, Drive, Photos, YouTube — because an automated system flagged one of his images. No human reviewed the decision. His appeal was denied by another automated system. He lost years of client work, personal memories, and business correspondence. Google eventually restored access after the story went viral. Most people who experience the same thing don't have that luck.

This isn't an isolated case. Apple locks accounts based on automated fraud detection with no clear appeal path. Microsoft has disabled Outlook accounts over suspected terms-of-service violations that turned out to be false positives. Facebook routinely locks users out during "identity verification" processes that can take weeks. In each case, the pattern is the same: a corporation makes a unilateral decision about your access to your own data, and your only recourse is to hope their support system eventually reaches a human who cares.

The fundamental problem isn't that these systems are poorly designed. It's that they can't be well designed. Any system where a single entity controls both your data and your access to it is structurally fragile. The entity has incentives that don't align with yours. It has scale constraints that prevent human review. It has legal obligations that may require it to lock your account without explanation. No amount of engineering fixes this. The architecture is wrong.

## The recovery problem in local-first systems

If you accept that local-first storage — where your data lives on your own devices, encrypted with your own keys — is the right architecture for personal data, then you inherit a different problem: what happens when you lose all your devices?

With cloud services, the answer is trivial. Log in from any browser, and your data is there. This is the genuine advantage of the cloud model, and dismissing it is dishonest. If your house burns down and you lose every device you own, your Google account still works. Your iCloud photos are still there.

A local-first system that offers no recovery from catastrophic loss is asking users to accept a real risk in exchange for a philosophical principle. Most people won't, and they shouldn't have to. The question is whether we can provide recovery without reintroducing the single point of control that makes cloud systems fragile.

## Shamir's gift

In 1979, Adi Shamir published a paper describing a method for splitting a secret into multiple pieces such that any subset of a specified size can reconstruct the original, but any smaller subset reveals nothing. The mathematics is elegant: the secret is encoded as the constant term of a random polynomial, and each shard is a point on that polynomial. With enough points, you can reconstruct the polynomial and recover the constant term. With fewer points than the threshold, the secret is information-theoretically secure — not just computationally hard to break, but literally impossible to derive.

This is the foundation of guardian recovery in Sovereign OS.

## How it works

You choose five people you trust. Your master recovery key — the key from which all other keys in the system can be derived — is split into five shards using Shamir's Secret Sharing with a 3-of-5 threshold. Each shard is encrypted with the guardian's public key before distribution. The guardian receives an opaque blob. They don't know what it contains. They don't need to install any software. They store it however they like.

If you lose all your devices, you initiate recovery on a new device. You enter a passphrase that you memorized when you first set up the system — this passphrase is never stored digitally, and it serves as a second factor to prevent impersonation. The system contacts all five guardians.

Then: a 72-hour waiting period. All five guardians are notified that a recovery has been initiated — not just the three being asked to participate. This is the anti-fraud mechanism. If someone has stolen your identity and is attempting recovery, the real you will be notified through at least one guardian and can abort the process. The 72-hour window gives you time to respond even if you're traveling, sleeping, or otherwise unreachable.

After the waiting period, three or more guardians approve the recovery. They verify your identity through whatever means they find appropriate — a phone call, a video chat, a question only you'd know the answer to. The system doesn't prescribe how guardians verify identity, because the right method depends on the relationship. Your mother verifies differently than your business partner. This flexibility is a feature, not a limitation.

Once three shards are submitted and the passphrase is verified, the master key is reconstructed. You set a new master key, all old shards are invalidated, and new shards are distributed to your guardians. The old key is gone.

## Why five? Why three?

The 3-of-5 threshold is a balance between redundancy and security. With 5 guardians and a threshold of 3, you can lose contact with two guardians — they move, they die, they lose their shard — and still recover. But an attacker needs to compromise three people who know you personally, plus obtain your memorized passphrase, plus wait 72 hours without any of the five guardians raising an alarm. The difficulty of this attack scales with the strength of your relationships, not with the strength of a corporation's security team.

What if guardians collude? This is the obvious objection, and it's worth taking seriously. Three of your five guardians would need to conspire to steal your master key, obtain your memorized passphrase (which they don't have), and execute the recovery without the other two guardians aborting the process. If you've chosen your guardians well — a mix of family, friends, and trusted colleagues, distributed across social circles — collusion requires a conspiracy across independent relationships. Not impossible. But harder than compromising a single corporate account, and you have the ability to choose your own threat model by choosing your guardians wisely.

The system also supports guardian rotation. If a relationship deteriorates, you revoke that guardian's shard and designate a replacement. The old shard becomes useless. The threshold is never reduced — you redistribute to maintain 3-of-5. Annual rotation is recommended, not because the cryptography weakens, but because relationships change.

## Trust in humans, not institutions

There is something philosophically significant about this model. Cloud account recovery places trust in an institution: a corporation with millions of users, automated systems, and policies optimized for scale. Guardian recovery places trust in specific humans: people who know you, who have a relationship with you, who can verify your identity not through bureaucratic checkboxes but through personal knowledge.

This is closer to how trust actually works in human life. When something goes wrong — when you're locked out, when you've lost everything — you don't want an algorithm deciding whether you're really you. You want someone who recognizes your voice, who remembers your shared history, who can make a judgment call based on context that no automated system could evaluate.

Sovereign OS is built on a bet about human nature that I've articulated before in other contexts: that trusting people — specific, carefully chosen people — is both more resilient and more humane than trusting institutions that have no capacity to know you. The guardian model reflects this conviction. It's not trustless. It's trust-ful, in a deliberate and structured way.

The worst case under this model — all five guardians become unreachable, and you've forgotten your passphrase — is data loss. The worst case under the cloud model — the corporation locks your account — is also data loss, but with the additional indignity of your data still existing on someone else's servers, inaccessible to you but not necessarily to them.

I know which failure mode I prefer.

*Sovereign OS guardian recovery is implemented in `sovereign-crypto/src/shamir.rs`. The system is open source under AGPL-3.0. GitHub: [link]*

*Co-developed by Céline Lenoble and Claude Opus 4.6 (Anthropic).*
