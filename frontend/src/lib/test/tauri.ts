/**
 * Test helpers for mocking Tauri IPC commands.
 *
 * The mock layer is installed globally via vitest-setup.ts. By default every
 * `invoke()` call throws — register a handler per command before driving code
 * that calls it.
 *
 * @example
 * import { mockTauriCommand } from '$lib/test/tauri';
 *
 * mockTauriCommand('canvas_load', () => ({
 *   documents: [],
 *   threads: [],
 *   relationships: [],
 *   milestones: []
 * }));
 *
 * await load();
 * expect(canvas.loaded).toBe(true);
 */
export function mockTauriCommand<TArgs = unknown, TResult = unknown>(
	cmd: string,
	handler: (args: TArgs) => TResult | Promise<TResult>
): void {
	globalThis.__tauriHandlers.set(
		cmd,
		handler as (args: unknown) => unknown
	);
}
