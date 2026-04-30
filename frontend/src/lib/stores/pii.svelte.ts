/** Rune-based reactive state for the PII management dashboard.
 *
 * Mirrors `contacts.svelte.ts` shape. Holds the entity list, the
 * record list (filtered + cached), and the current selection so the
 * three-column dashboard layout can render reactively.
 */

import {
	listPiiEntities,
	listPiiRecords,
	listShareRecordsForEntity,
	type PiiEntity,
	type PiiRecord,
	type ReviewStateString,
	type ShareRecord
} from '$lib/api/commands';

/** Reactive PII state. */
export const piiState = $state({
	entities: [] as PiiEntity[],
	records: [] as PiiRecord[],
	loaded: false,
	/** Currently-selected entity in the left column. */
	selectedEntityId: null as string | null,
	/** Active tab in the entity-detail center column. */
	activeTab: 'inventory' as 'inventory' | 'vault' | 'shared' | 'cookies',
	/** Lazily-loaded ShareRecord list per entity ID. */
	shareRecordsByEntity: {} as Record<string, ShareRecord[]>,
	/** Active signup-capture extraction. Set when the user clicks
	 *  "Save credentials" in the BrowserPanel and the
	 *  `browser-form-extracted` event arrives. */
	signupCapture: null as import('$lib/api/commands').BrowserFormExtraction | null
});

/** Load all entities + all records in one parallel fetch. */
export async function loadPii() {
	try {
		const [entities, records] = await Promise.all([
			listPiiEntities(),
			listPiiRecords()
		]);
		piiState.entities = entities;
		piiState.records = records;
		piiState.loaded = true;
	} catch (e) {
		console.error('Failed to load PII state:', e);
	}
}

/** Re-fetch records only (entities don't change as often). */
export async function refreshPiiRecords() {
	try {
		piiState.records = await listPiiRecords();
	} catch (e) {
		console.error('Failed to refresh PII records:', e);
	}
}

/** Records for one entity (filters in-memory; cheap). */
export function recordsForEntity(entityId: string | null): PiiRecord[] {
	if (entityId === null) return piiState.records;
	return piiState.records.filter((r) => r.entity_id === entityId);
}

/** Records by review state. */
export function recordsByState(state: ReviewStateString): PiiRecord[] {
	return piiState.records.filter((r) => r.review_state === state);
}

/** Discovered findings (non-vault) for an entity. */
export function inventoryForEntity(entityId: string | null): PiiRecord[] {
	return recordsForEntity(entityId).filter(
		(r) => !r.stored_secret && r.review_state !== 'dismissed'
	);
}

/** Vault entries (user-stored secrets) for an entity. */
export function vaultForEntity(entityId: string | null): PiiRecord[] {
	return recordsForEntity(entityId).filter((r) => r.stored_secret);
}

/** Per-entity record count, used to sort the entity list. */
export function piiCountForEntity(entityId: string): number {
	return piiState.records.filter(
		(r) => r.entity_id === entityId && r.review_state !== 'dismissed'
	).length;
}

/** Total Unreviewed records across all entities — used for the
 *  taskbar badge and the right-column queue header. */
export function unreviewedCount(): number {
	return piiState.records.filter((r) => r.review_state === 'unreviewed').length;
}

/** Lazily fetch the ShareRecord list for one entity. Caches per ID;
 *  call `refreshShareRecordsForEntity` to bypass the cache. */
export async function loadShareRecordsForEntity(entityId: string) {
	if (entityId in piiState.shareRecordsByEntity) return;
	try {
		const records = await listShareRecordsForEntity(entityId);
		piiState.shareRecordsByEntity = {
			...piiState.shareRecordsByEntity,
			[entityId]: records
		};
	} catch (e) {
		console.error('Failed to load share records:', e);
	}
}

/** Force a re-fetch of an entity's share records, ignoring the cache. */
export async function refreshShareRecordsForEntity(entityId: string) {
	try {
		const records = await listShareRecordsForEntity(entityId);
		piiState.shareRecordsByEntity = {
			...piiState.shareRecordsByEntity,
			[entityId]: records
		};
	} catch (e) {
		console.error('Failed to refresh share records:', e);
	}
}

/** Look up a record's PiiKind by ID, used by the Shared tab to
 *  render "Shared <kind> on <date>…". Returns 'other' if the record
 *  isn't in the cache. */
export function kindForRecordId(recordId: string): string {
	return piiState.records.find((r) => r.id === recordId)?.kind ?? 'other';
}
