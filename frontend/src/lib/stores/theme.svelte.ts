/** Rune-based reactive state for theming. */

import { themes, type ThemeName } from '$lib/theme/colors';

/** Reactive theme state. */
export const theme = $state({ current: 'dark' as ThemeName });

/** Apply the theme's CSS variables to :root. */
export function applyTheme(name: ThemeName) {
	const vars = themes[name];
	const root = document.documentElement;
	for (const [prop, value] of Object.entries(vars)) {
		root.style.setProperty(prop, value);
	}
	theme.current = name;
}
