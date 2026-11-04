/**
 * v0.0.12 item 12, P4 — "Said where it is seen" (doc/plans/V0012-item12-phone-p2p.md).
 * Sentence amended on PANDA2's second-position read: the earlier "Syncs on the local
 * network only on this phone." said less than P1/P3 guarantee.
 *
 * While the phone's P2P node runs without the DNS transport (no system resolver
 * config on Android), Settings → Devices shows one line:
 *   "This phone cannot look up names: it syncs on the local network and with addresses given by IP."
 *
 * API this test expects (does not exist yet): `get_p2p_settings` (P2pSettings,
 * `$lib/api/commands`) carries `without_dns: boolean` — true only while the node
 * runs without DNS (the app sets it from `SovereignNode::has_dns_transport()`).
 */
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { render, fireEvent, waitFor } from '@testing-library/svelte';
import { tick } from 'svelte';
import { mockTauriCommand } from '$lib/test/tauri';
import { app } from '$lib/stores/app.svelte';
import SettingsPanel from './SettingsPanel.svelte';

const LINE = 'This phone cannot look up names: it syncs on the local network and with addresses given by IP.';

function mockSettings(p2p: Record<string, unknown>) {
	mockTauriCommand('get_profile', () => ({
		user_id: 'u',
		designation: 'Me',
		nickname: null,
		bubble_style: 'icon',
		display_name: null
	}));
	mockTauriCommand('get_config', () => ({
		ai_model_dir: '',
		ai_router_model: '',
		ai_reasoning_model: '',
		ai_n_gpu_layers: 0,
		ai_n_ctx: 2048,
		ai_prompt_format: 'chatml',
		crypto_enabled: true,
		crypto_keystroke_enabled: false,
		crypto_max_login_attempts: 10,
		crypto_lockout_seconds: 300,
		ui_theme: 'dark'
	}));
	mockTauriCommand('list_trash', () => []);
	mockTauriCommand('list_paired_devices', () => []);
	mockTauriCommand('get_local_peer_id', () => '12D3KooWPhone');
	mockTauriCommand('get_p2p_settings', () => ({
		available: true,
		enabled: true,
		device_name: 'Pixel',
		enable_mdns: true,
		wifi_only: true,
		running: true,
		...p2p
	}));
}

async function openDevices() {
	const view = render(SettingsPanel);
	app.settingsVisible = true;
	await tick();
	const tab = await view.findByRole('button', { name: 'Devices' });
	await fireEvent.click(tab);
	// The devices tab has loaded once the P2P configuration block is shown.
	await view.findByText('P2P configuration');
	return view;
}

beforeEach(() => {
	app.settingsVisible = false;
});

afterEach(() => {
	app.settingsVisible = false;
});

describe('Settings → Devices — the node runs without DNS (item 12, P4)', () => {
	it('P4: shows "This phone cannot look up names: it syncs on the local network and with addresses given by IP." when the backend reports no DNS', async () => {
		mockSettings({ without_dns: true });
		const view = await openDevices();
		await waitFor(() => expect(view.getByText(LINE)).toBeInTheDocument());
		// One line, not repeated.
		expect(view.getAllByText(LINE)).toHaveLength(1);
	});

	it('P4: says nothing of the kind where the node has DNS (desktop, as today)', async () => {
		mockSettings({ without_dns: false });
		const view = await openDevices();
		expect(view.queryByText(LINE)).toBeNull();
	});

	it('P4: says nothing when the backend does not report the field (older backend)', async () => {
		mockSettings({});
		const view = await openDevices();
		expect(view.queryByText(LINE)).toBeNull();
	});
});
