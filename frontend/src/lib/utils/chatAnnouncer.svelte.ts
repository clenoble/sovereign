/** What a screen reader hears from the chat (v0.0.12, F-a11y, F2,
 *  `doc/plans/V0012-frontend-a11y.md`).
 *
 *  The conversation itself is a `role="log"` with `aria-live="off"`: it holds
 *  every message, the person's own included, and reads none of them out. A
 *  separate visually hidden `aria-live="polite"` region shows `announcement`,
 *  which this tracker sets to each new assistant or system message:
 *  - only additions: a list that extends the one seen before. The messages
 *    present when the component mounts, and a list replaced by one that does
 *    not extend it ("restored"), announce nothing;
 *  - never the person's own messages: they have just typed them;
 *  - counted on `chat.messages` itself, never on the 200-capped
 *    `recentMessages()`, whose length stops growing at 200.
 *
 *  `seq` changes with each announcement, so the region can key its text and
 *  a message identical to the previous one is still read (same text, new node).
 *
 *  Call during a component's initialisation (it registers an `$effect`).
 */
import { chat, type ChatMessage } from '$lib/stores/chat.svelte';

export function trackChatAnnouncements() {
	const state = $state({ announcement: '', seq: 0 });

	// What was seen last: its length and its last message. Plain variables,
	// not state: the effect compares against them, it does not depend on them.
	let seenLength = chat.messages.length;
	let seenLast: ChatMessage | undefined = chat.messages[seenLength - 1];
	let seq = 0;

	$effect(() => {
		const now = chat.messages;
		const extendsSeen =
			now.length > seenLength && (seenLength === 0 || now[seenLength - 1] === seenLast);
		const added = extendsSeen ? now.slice(seenLength) : [];
		seenLength = now.length;
		seenLast = now[now.length - 1];

		const said = added.filter((m) => m.role !== 'user').map((m) => m.text);
		if (said.length > 0) {
			state.announcement = said.join('\n');
			state.seq = ++seq; // written, not read: the effect does not depend on it
		}
	});

	return state;
}
