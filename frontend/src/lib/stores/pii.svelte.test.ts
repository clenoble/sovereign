import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mockTauriCommand } from '$lib/test/tauri';
import type {
	BrowserCookie,
	PiiEntity,
	PiiRecord,
	ShareRecord
} from '$lib/api/commands';
import {
	piiState,
	loadPii,
	refreshPiiRecords,
	recordsForEntity,
	recordsByState,
	inventoryForEntity,
	vaultForEntity,
	piiCountForEntity,
	unreviewedCount,
	loadShareRecordsForEntity,
	refreshShareRecordsForEntity,
	loadCookiesForEntity,
	refreshCookiesForEntity,
	kindForRecordId
} from './pii.svelte';

// ─────────────────────────────────────────────────────────────────────
// Fixtures

function makeEntity(id: string, name = `Entity ${id}`, kind: PiiEntity['kind'] = 'org'): PiiEntity {
	return {
		id,
		name,
		kind,
		domains: [],
		contact_ids: [],
		notes: '',
		is_owned: false,
		created_at: '2026-04-01T00:00:00Z',
		modified_at: '2026-04-01T00:00:00Z'
	};
}

function makeRecord(
	id: string,
	overrides: Partial<PiiRecord> = {}
): PiiRecord {
	return {
		id,
		kind: 'email',
		label: null,
		entity_id: null,
		stored_secret: false,
		confidence: 0.9,
		review_state: 'unreviewed',
		discovered_at: '2026-04-15T00:00:00Z',
		last_revealed_at: null,
		use_count: 0,
		sources: [],
		...overrides
	};
}

function makeShareRecord(id: string, piiRecordId: string, toEntity: string): ShareRecord {
	return {
		id,
		pii_record_id: piiRecordId,
		to_entity_id: toEntity,
		via_message_id: null,
		via_url: null,
		shared_at: '2026-04-20T10:00:00Z',
		channel: 'web'
	};
}

function makeCookie(name: string, domain = 'example.com'): BrowserCookie {
	return {
		name,
		value: `${name}-value`,
		domain,
		path: '/',
		expires: null,
		http_only: false,
		secure: true,
		same_site: 'lax'
	};
}

// ─────────────────────────────────────────────────────────────────────
// Reset store between tests so each one starts from a clean slate.

beforeEach(() => {
	piiState.entities = [];
	piiState.records = [];
	piiState.loaded = false;
	piiState.selectedEntityId = null;
	piiState.activeTab = 'inventory';
	piiState.shareRecordsByEntity = {};
	piiState.signupCapture = null;
	piiState.autofillRequested = false;
	piiState.autofillExtraction = null;
	piiState.cookiesByEntity = {};
});

// ─────────────────────────────────────────────────────────────────────
// loadPii

describe('loadPii', () => {
	it('populates entities + records and flips loaded=true on success', async () => {
		const ents = [makeEntity('e:1'), makeEntity('e:2')];
		const recs = [makeRecord('p:1'), makeRecord('p:2')];
		mockTauriCommand('list_pii_entities', () => ents);
		mockTauriCommand('list_pii_records', () => recs);

		await loadPii();

		expect(piiState.entities).toEqual(ents);
		expect(piiState.records).toEqual(recs);
		expect(piiState.loaded).toBe(true);
	});

	it('runs both fetches in parallel (Promise.all)', async () => {
		const order: string[] = [];
		mockTauriCommand('list_pii_entities', async () => {
			order.push('entities-start');
			await Promise.resolve();
			order.push('entities-end');
			return [];
		});
		mockTauriCommand('list_pii_records', async () => {
			order.push('records-start');
			await Promise.resolve();
			order.push('records-end');
			return [];
		});

		await loadPii();

		// Both starts happen before either end → confirms concurrent dispatch.
		expect(order.indexOf('entities-start')).toBeLessThan(order.indexOf('records-end'));
		expect(order.indexOf('records-start')).toBeLessThan(order.indexOf('entities-end'));
	});

	it('swallows errors and leaves loaded=false (logs to console.error)', async () => {
		const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
		mockTauriCommand('list_pii_entities', () => {
			throw new Error('boom');
		});
		mockTauriCommand('list_pii_records', () => []);

		await loadPii();

		expect(piiState.loaded).toBe(false);
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});
});

// ─────────────────────────────────────────────────────────────────────
// refreshPiiRecords

describe('refreshPiiRecords', () => {
	it('replaces records and leaves entities untouched', async () => {
		piiState.entities = [makeEntity('e:keep')];
		piiState.records = [makeRecord('p:old')];

		const fresh = [makeRecord('p:new1'), makeRecord('p:new2')];
		mockTauriCommand('list_pii_records', () => fresh);

		await refreshPiiRecords();

		expect(piiState.records).toEqual(fresh);
		expect(piiState.entities).toEqual([makeEntity('e:keep')]);
	});

	it('swallows errors silently', async () => {
		const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
		const original = [makeRecord('p:1')];
		piiState.records = original;
		mockTauriCommand('list_pii_records', () => {
			throw new Error('network');
		});

		await refreshPiiRecords();

		// Records left unchanged on error
		expect(piiState.records).toEqual(original);
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});
});

// ─────────────────────────────────────────────────────────────────────
// recordsForEntity

describe('recordsForEntity', () => {
	beforeEach(() => {
		piiState.records = [
			makeRecord('p:1', { entity_id: 'e:1' }),
			makeRecord('p:2', { entity_id: 'e:2' }),
			makeRecord('p:3', { entity_id: 'e:1' }),
			makeRecord('p:4', { entity_id: null })
		];
	});

	it('returns ALL records when entityId is null (unfiltered)', () => {
		expect(recordsForEntity(null)).toHaveLength(4);
	});

	it('filters to records whose entity_id matches', () => {
		const result = recordsForEntity('e:1');
		expect(result.map((r) => r.id)).toEqual(['p:1', 'p:3']);
	});

	it('returns [] when no records match the entity', () => {
		expect(recordsForEntity('e:nonexistent')).toEqual([]);
	});

	it('does NOT match records whose entity_id is null when given a string id', () => {
		// Records with entity_id=null should not appear under any specific entity.
		const result = recordsForEntity('e:2');
		expect(result.map((r) => r.id)).toEqual(['p:2']);
	});
});

// ─────────────────────────────────────────────────────────────────────
// recordsByState

describe('recordsByState', () => {
	beforeEach(() => {
		piiState.records = [
			makeRecord('p:1', { review_state: 'unreviewed' }),
			makeRecord('p:2', { review_state: 'confirmed' }),
			makeRecord('p:3', { review_state: 'unreviewed' }),
			makeRecord('p:4', { review_state: 'dismissed' })
		];
	});

	it('returns only unreviewed records', () => {
		expect(recordsByState('unreviewed').map((r) => r.id)).toEqual(['p:1', 'p:3']);
	});

	it('returns only confirmed records', () => {
		expect(recordsByState('confirmed').map((r) => r.id)).toEqual(['p:2']);
	});

	it('returns only dismissed records', () => {
		expect(recordsByState('dismissed').map((r) => r.id)).toEqual(['p:4']);
	});
});

// ─────────────────────────────────────────────────────────────────────
// inventoryForEntity / vaultForEntity

describe('inventoryForEntity', () => {
	beforeEach(() => {
		piiState.records = [
			// Discovered, confirmed → inventory ✓
			makeRecord('disc:1', { entity_id: 'e:1', stored_secret: false, review_state: 'confirmed' }),
			// Discovered, unreviewed → inventory ✓
			makeRecord('disc:2', { entity_id: 'e:1', stored_secret: false, review_state: 'unreviewed' }),
			// Discovered, dismissed → NOT inventory (filtered out)
			makeRecord('disc:3', { entity_id: 'e:1', stored_secret: false, review_state: 'dismissed' }),
			// Vault entry → NOT inventory
			makeRecord('vault:1', { entity_id: 'e:1', stored_secret: true, review_state: 'confirmed' }),
			// Wrong entity → NOT included
			makeRecord('disc:4', { entity_id: 'e:2', stored_secret: false, review_state: 'unreviewed' })
		];
	});

	it('returns only non-stored, non-dismissed records for the entity', () => {
		const result = inventoryForEntity('e:1');
		expect(result.map((r) => r.id).sort()).toEqual(['disc:1', 'disc:2']);
	});

	it('returns inventory across all entities when entityId is null', () => {
		const result = inventoryForEntity(null);
		// disc:1, disc:2, disc:4 (everything except dismissed disc:3 and vault:1)
		expect(result.map((r) => r.id).sort()).toEqual(['disc:1', 'disc:2', 'disc:4']);
	});
});

describe('vaultForEntity', () => {
	beforeEach(() => {
		piiState.records = [
			makeRecord('disc:1', { entity_id: 'e:1', stored_secret: false }),
			makeRecord('vault:1', { entity_id: 'e:1', stored_secret: true }),
			makeRecord('vault:2', { entity_id: 'e:1', stored_secret: true }),
			makeRecord('vault:3', { entity_id: 'e:2', stored_secret: true })
		];
	});

	it('returns only stored_secret=true records for the entity', () => {
		const result = vaultForEntity('e:1');
		expect(result.map((r) => r.id).sort()).toEqual(['vault:1', 'vault:2']);
	});

	it('includes dismissed vault entries (vault has no review-state filter)', () => {
		piiState.records = [
			makeRecord('vault:dismissed', {
				entity_id: 'e:1',
				stored_secret: true,
				review_state: 'dismissed'
			})
		];
		// This guards the documented difference between inventory (excludes
		// dismissed) and vault (includes everything stored).
		expect(vaultForEntity('e:1').map((r) => r.id)).toEqual(['vault:dismissed']);
	});
});

// ─────────────────────────────────────────────────────────────────────
// piiCountForEntity / unreviewedCount

describe('piiCountForEntity', () => {
	it('counts non-dismissed records belonging to the entity', () => {
		piiState.records = [
			makeRecord('p:1', { entity_id: 'e:1', review_state: 'confirmed' }),
			makeRecord('p:2', { entity_id: 'e:1', review_state: 'unreviewed' }),
			makeRecord('p:3', { entity_id: 'e:1', review_state: 'dismissed' }), // excluded
			makeRecord('p:4', { entity_id: 'e:2', review_state: 'confirmed' })  // wrong entity
		];
		expect(piiCountForEntity('e:1')).toBe(2);
		expect(piiCountForEntity('e:2')).toBe(1);
		expect(piiCountForEntity('e:nope')).toBe(0);
	});
});

describe('unreviewedCount', () => {
	it('counts records in the unreviewed state across all entities', () => {
		piiState.records = [
			makeRecord('p:1', { review_state: 'unreviewed' }),
			makeRecord('p:2', { review_state: 'confirmed' }),
			makeRecord('p:3', { review_state: 'unreviewed' }),
			makeRecord('p:4', { review_state: 'dismissed' })
		];
		expect(unreviewedCount()).toBe(2);
	});

	it('returns 0 when no records exist', () => {
		expect(unreviewedCount()).toBe(0);
	});
});

// ─────────────────────────────────────────────────────────────────────
// loadShareRecordsForEntity (caches per entity)

describe('loadShareRecordsForEntity', () => {
	it('fetches and caches share records under the entity ID', async () => {
		const recs = [makeShareRecord('s:1', 'p:1', 'e:1')];
		const handler = vi.fn(() => recs);
		mockTauriCommand('list_share_records_for_entity', handler);

		await loadShareRecordsForEntity('e:1');

		expect(piiState.shareRecordsByEntity['e:1']).toEqual(recs);
		expect(handler).toHaveBeenCalledTimes(1);
	});

	it('does NOT re-fetch if the entity is already in the cache (idempotent)', async () => {
		piiState.shareRecordsByEntity = { 'e:1': [makeShareRecord('cached', 'p:1', 'e:1')] };
		const handler = vi.fn(() => []);
		mockTauriCommand('list_share_records_for_entity', handler);

		await loadShareRecordsForEntity('e:1');

		// Cache untouched, no refetch
		expect(piiState.shareRecordsByEntity['e:1']).toEqual([
			makeShareRecord('cached', 'p:1', 'e:1')
		]);
		expect(handler).not.toHaveBeenCalled();
	});

	it('caches an empty list (next call still skips the fetch)', async () => {
		// Important: empty array is a valid cache hit, not a cache miss.
		const handler = vi.fn(() => []);
		mockTauriCommand('list_share_records_for_entity', handler);

		await loadShareRecordsForEntity('e:1');
		await loadShareRecordsForEntity('e:1');

		expect(handler).toHaveBeenCalledTimes(1);
		expect(piiState.shareRecordsByEntity['e:1']).toEqual([]);
	});

	it('swallows errors and leaves the cache unset for that entity', async () => {
		const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
		mockTauriCommand('list_share_records_for_entity', () => {
			throw new Error('fail');
		});

		await loadShareRecordsForEntity('e:1');

		expect(piiState.shareRecordsByEntity['e:1']).toBeUndefined();
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});
});

describe('refreshShareRecordsForEntity', () => {
	it('bypasses the cache and overwrites it with fresh results', async () => {
		piiState.shareRecordsByEntity = { 'e:1': [makeShareRecord('stale', 'p:1', 'e:1')] };
		const fresh = [makeShareRecord('fresh', 'p:9', 'e:1')];
		mockTauriCommand('list_share_records_for_entity', () => fresh);

		await refreshShareRecordsForEntity('e:1');

		expect(piiState.shareRecordsByEntity['e:1']).toEqual(fresh);
	});

	it('swallows errors and leaves stale cache in place', async () => {
		const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
		const stale = [makeShareRecord('stale', 'p:1', 'e:1')];
		piiState.shareRecordsByEntity = { 'e:1': stale };
		mockTauriCommand('list_share_records_for_entity', () => {
			throw new Error('fail');
		});

		await refreshShareRecordsForEntity('e:1');

		expect(piiState.shareRecordsByEntity['e:1']).toEqual(stale);
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});
});

// ─────────────────────────────────────────────────────────────────────
// kindForRecordId

describe('kindForRecordId', () => {
	beforeEach(() => {
		piiState.records = [
			makeRecord('p:1', { kind: 'email' }),
			makeRecord('p:2', { kind: 'password' })
		];
	});

	it('returns the kind for a known record', () => {
		expect(kindForRecordId('p:1')).toBe('email');
		expect(kindForRecordId('p:2')).toBe('password');
	});

	it('returns "other" for an unknown record (the documented fallback)', () => {
		expect(kindForRecordId('p:nope')).toBe('other');
	});
});

// ─────────────────────────────────────────────────────────────────────
// loadCookiesForEntity / refreshCookiesForEntity

describe('loadCookiesForEntity', () => {
	it('fetches and caches cookies under the entity ID', async () => {
		const cookies = [makeCookie('session'), makeCookie('csrf')];
		const handler = vi.fn(() => cookies);
		mockTauriCommand('list_cookies_for_entity', handler);

		await loadCookiesForEntity('e:1');

		expect(piiState.cookiesByEntity['e:1']).toEqual(cookies);
		expect(handler).toHaveBeenCalledTimes(1);
	});

	it('does NOT re-fetch when the entity is already cached', async () => {
		piiState.cookiesByEntity = { 'e:1': [makeCookie('cached')] };
		const handler = vi.fn(() => []);
		mockTauriCommand('list_cookies_for_entity', handler);

		await loadCookiesForEntity('e:1');

		expect(handler).not.toHaveBeenCalled();
	});

	it('caches an empty cookie list (subsequent calls skip)', async () => {
		const handler = vi.fn(() => []);
		mockTauriCommand('list_cookies_for_entity', handler);

		await loadCookiesForEntity('e:1');
		await loadCookiesForEntity('e:1');

		expect(handler).toHaveBeenCalledTimes(1);
	});

	it('swallows errors silently', async () => {
		const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
		mockTauriCommand('list_cookies_for_entity', () => {
			throw new Error('no browser');
		});

		await loadCookiesForEntity('e:1');

		expect(piiState.cookiesByEntity['e:1']).toBeUndefined();
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});
});

describe('refreshCookiesForEntity', () => {
	it('overwrites the cache with a fresh fetch', async () => {
		piiState.cookiesByEntity = { 'e:1': [makeCookie('stale')] };
		const fresh = [makeCookie('fresh')];
		mockTauriCommand('list_cookies_for_entity', () => fresh);

		await refreshCookiesForEntity('e:1');

		expect(piiState.cookiesByEntity['e:1']).toEqual(fresh);
	});

	it('swallows errors and leaves the existing cache in place', async () => {
		const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
		const stale = [makeCookie('stale')];
		piiState.cookiesByEntity = { 'e:1': stale };
		mockTauriCommand('list_cookies_for_entity', () => {
			throw new Error('fail');
		});

		await refreshCookiesForEntity('e:1');

		expect(piiState.cookiesByEntity['e:1']).toEqual(stale);
		expect(errSpy).toHaveBeenCalled();
		errSpy.mockRestore();
	});
});
