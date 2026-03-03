/** Rune-based reactive state for contacts and inbox. */

import { listContacts, type ContactSummaryDto } from '$lib/api/commands';

/** Reactive contacts state. */
export const contactsState = $state({
	contacts: [] as ContactSummaryDto[],
	loaded: false
});

/** Load contacts from backend. */
export async function loadContacts() {
	try {
		const result = await listContacts();
		result.sort((a, b) => b.unread_count - a.unread_count);
		contactsState.contacts = result;
		contactsState.loaded = true;
	} catch (e) {
		console.error('Failed to load contacts:', e);
	}
}

/** Refresh contacts (re-fetch). */
export async function refreshContacts() {
	try {
		const result = await listContacts();
		result.sort((a, b) => b.unread_count - a.unread_count);
		contactsState.contacts = result;
	} catch (e) {
		console.error('Failed to refresh contacts:', e);
	}
}
