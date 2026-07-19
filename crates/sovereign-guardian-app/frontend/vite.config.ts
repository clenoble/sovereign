import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri desktop app frontend. Static SPA built to `dist`, embedded by the
// Rust crate via tauri.conf.json `frontendDist`.
export default defineConfig({
	plugins: [svelte()],
	clearScreen: false,
	server: { port: 5175, strictPort: true },
	build: { target: 'esnext', outDir: 'dist', emptyOutDir: true }
});
