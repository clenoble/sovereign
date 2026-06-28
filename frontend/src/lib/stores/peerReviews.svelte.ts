/** Peer-write review store (p2p-no-per-doc-authz) — Svelte 5 rune store. */

import type { PeerReviewDto } from '$lib/api/commands';

/** Parsed `PeerChangeVerdict` from the backend audit. */
export interface PeerVerdict {
	risk: string; // "low" | "medium" | "high"
	injection_detected: boolean;
	wholesale_rewrite: boolean;
	llm_assessed: boolean;
	summary: string;
	reasons: string[];
}

export interface PeerReview {
	kind: string; // "document" | "row"
	id: string;
	title: string;
	peer: string;
	at: string | null;
	assessment: PeerVerdict | null;
	canRestore: boolean;
}

/** Map a backend DTO into a UI review, parsing the JSON assessment. */
export function fromDto(d: PeerReviewDto): PeerReview {
	let assessment: PeerVerdict | null = null;
	if (d.assessment) {
		try {
			assessment = JSON.parse(d.assessment) as PeerVerdict;
		} catch {
			assessment = null;
		}
	}
	return {
		kind: d.kind,
		id: d.id,
		title: d.title,
		peer: d.peer,
		at: d.at,
		assessment,
		canRestore: d.can_restore
	};
}

export const peerReviews = $state({
	pending: [] as PeerReview[],
	visible: false
});

export function setPeerReviews(list: PeerReview[]) {
	peerReviews.pending = list;
}

/** Remove one review (kind + id together — ids are namespaced per kind). */
export function removePeerReview(kind: string, id: string) {
	peerReviews.pending = peerReviews.pending.filter((r) => !(r.kind === kind && r.id === id));
}

export function togglePeerReviews() {
	peerReviews.visible = !peerReviews.visible;
}
