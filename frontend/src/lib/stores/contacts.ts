/** Store for contacts and inbox state. */

import { writable, derived } from 'svelte/store';
import { listContacts, type ContactSummaryDto } from '$lib/api/commands';

function createContactsStore() {
	const { subscribe, update, set } = writable<{
		contacts: ContactSummaryDto[];
		loaded: boolean;
	}>({
		contacts: [],
		loaded: false
	});

	return {
		subscribe,

		/** Load contacts from backend. */
		async load() {
			try {
				const result = await listContacts();
				// Sort by unread count descending
				result.sort((a, b) => b.unread_count - a.unread_count);
				set({ contacts: result, loaded: true });
			} catch (e) {
				console.error('Failed to load contacts:', e);
			}
		},

		/** Refresh contacts (re-fetch). */
		async refresh() {
			try {
				const result = await listContacts();
				result.sort((a, b) => b.unread_count - a.unread_count);
				update((s) => ({ ...s, contacts: result }));
			} catch (e) {
				console.error('Failed to refresh contacts:', e);
			}
		}
	};
}

export const contacts = createContactsStore();

/** Total unread count across all contacts. */
export const totalUnread = derived(contacts, ($c) =>
	$c.contacts.reduce((sum, c) => sum + c.unread_count, 0)
);
