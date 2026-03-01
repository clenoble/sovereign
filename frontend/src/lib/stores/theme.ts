import { writable } from 'svelte/store';
import { themes, type ThemeName } from '$lib/theme/colors';

export const currentTheme = writable<ThemeName>('dark');

/** Apply the theme's CSS variables to :root. */
export function applyTheme(name: ThemeName) {
	const vars = themes[name];
	const root = document.documentElement;
	for (const [prop, value] of Object.entries(vars)) {
		root.style.setProperty(prop, value);
	}
	currentTheme.set(name);
}
