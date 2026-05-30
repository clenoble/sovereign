import '@testing-library/jest-dom/vitest';
import { afterEach, vi } from 'vitest';

declare global {
	// eslint-disable-next-line no-var
	var __tauriHandlers: Map<string, (args: unknown) => unknown>;
}

const tauriHandlers = new Map<string, (args: unknown) => unknown>();
globalThis.__tauriHandlers = tauriHandlers;

vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn(async (cmd: string, args?: unknown) => {
		const handler = tauriHandlers.get(cmd);
		if (!handler) {
			throw new Error(
				`[tauri-mock] Unmocked Tauri command "${cmd}". ` +
					`Register with mockTauriCommand("${cmd}", (args) => ...) before invoking.`
			);
		}
		return handler(args);
	})
}));

vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(async () => () => {}),
	emit: vi.fn(async () => {}),
	once: vi.fn(async () => () => {})
}));

afterEach(() => {
	tauriHandlers.clear();
});
