/** v0.0.13 rc2 (Céline, 2026-11-20; the way decided by the Fable maille, by delegation of Céline, the
 *  same day; its seam proposed by the implementer before this test): on the phone, the page writes
 *  nothing to the system log. On Android the web view sends every console message and every uncaught
 *  error to the system log, which USB debugging, a bug report or a forensic tool can read.
 *
 *  The page's first code is `static/phone-silence.js`, a classic script, the first element of
 *  `<head>` in `src/app.html` (the CSP allows `script-src 'self'`, not an inline script). On a phone
 *  by its user agent (Android, iPhone, iPad, iPod), it makes every console method do nothing and
 *  stops uncaught errors and unhandled rejections from being reported. Anywhere else it does
 *  nothing. Nothing else of the page changes: its own listeners still hear an error.
 *
 *  These tests run the script's text against a page they make, and read `app.html`. What holds it on
 *  a candidate is the logcat measure on its APK (the release checklist). Written before the code, by
 *  another author than the implementer; the implementer does not edit this file. */
import { describe, expect, it, vi } from 'vitest';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const SCRIPT = resolve(process.cwd(), 'static/phone-silence.js');
const APP_HTML = resolve(process.cwd(), 'src/app.html');

const ANDROID =
	'Mozilla/5.0 (Linux; Android 14; sdk_gphone64_x86_64 Build/UE1A.230829.050; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/124.0.6367.219 Mobile Safari/537.36';
const IPHONE =
	'Mozilla/5.0 (iPhone; CPU iPhone OS 17_4 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148';
const WINDOWS =
	'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36 Edg/124.0.0.0';
const LINUX = 'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36';

const METHODS = ['log', 'info', 'warn', 'error', 'debug', 'trace'] as const;

function scriptText(): string {
	expect(existsSync(SCRIPT), 'static/phone-silence.js exists').toBe(true);
	return readFileSync(SCRIPT, 'utf8');
}

/** A page the script runs in: its window is an event target with its own console (each method a
 *  spy) and its own navigator; the script sees them as `window`, `self`, `globalThis`, `navigator`,
 *  `console` and `addEventListener`. */
function runIn(userAgent: string) {
	const spies = Object.fromEntries(METHODS.map((m) => [m, vi.fn()])) as Record<(typeof METHODS)[number], ReturnType<typeof vi.fn>>;
	const console = { ...spies } as unknown as Console;
	const events = new EventTarget();
	const navigator = { userAgent };
	const win = Object.assign(events, { console, navigator }) as unknown as Window & typeof globalThis;
	const addEventListener = events.addEventListener.bind(events);
	new Function('window', 'self', 'globalThis', 'navigator', 'console', 'addEventListener', scriptText())(
		win,
		win,
		win,
		navigator,
		console,
		addEventListener
	);
	const fire = (type: string) => {
		const e = new Event(type, { cancelable: true });
		events.dispatchEvent(e);
		return e;
	};
	return { console, spies, events, fire };
}

describe('on the phone, the page writes nothing to the system log', () => {
	for (const [name, ua] of [
		['Android', ANDROID],
		['iPhone', IPHONE]
	] as const) {
		it(`${name}: every console method does nothing`, () => {
			const p = runIn(ua);
			for (const m of METHODS) {
				p.console[m]('a word of the page');
				expect(p.spies[m], `console.${m} still reaches the original`).not.toHaveBeenCalled();
			}
		});

		it(`${name}: an uncaught error and an unhandled rejection are not reported`, () => {
			const p = runIn(ua);
			expect(p.fire('error').defaultPrevented, 'an uncaught error is reported').toBe(true);
			expect(p.fire('unhandledrejection').defaultPrevented, 'an unhandled rejection is reported').toBe(true);
		});
	}

	it("the page's own listeners still hear an error", () => {
		const p = runIn(ANDROID);
		const heard = vi.fn();
		p.events.addEventListener('error', heard);
		p.fire('error');
		expect(heard, "the page's own listener no longer hears an error").toHaveBeenCalledTimes(1);
	});
});

describe('anywhere else, nothing changes', () => {
	for (const [name, ua] of [
		['Windows', WINDOWS],
		['Linux', LINUX]
	] as const) {
		it(`${name}: the console still writes, and errors are still reported`, () => {
			const p = runIn(ua);
			for (const m of METHODS) {
				p.console[m]('a word of the page');
				expect(p.spies[m], `console.${m} on ${name}`).toHaveBeenCalledTimes(1);
			}
			expect(p.fire('error').defaultPrevented).toBe(false);
			expect(p.fire('unhandledrejection').defaultPrevented).toBe(false);
		});
	}
});

describe("the page's first code", () => {
	it("app.html loads /phone-silence.js as the page's first script, before SvelteKit's, a classic script (no module, async or defer)", () => {
		const html = readFileSync(APP_HTML, 'utf8');
		const firstScript = html.match(/<script\b[^>]*>/i);
		expect(firstScript, 'a script in app.html').not.toBeNull();
		const tag = firstScript![0];
		expect(tag, "the page's first script").toMatch(/\bsrc="\/phone-silence\.js"/);
		expect(tag, 'a classic script').not.toMatch(/\btype="module"|\basync\b|\bdefer\b/i);
		const head = html.indexOf('%sveltekit.head%');
		expect(head, 'app.html has %sveltekit.head%').toBeGreaterThan(-1);
		expect(html.indexOf(tag), "before SvelteKit's own scripts (%sveltekit.head%)").toBeLessThan(head);
		expect(html.indexOf(tag), 'in <head>').toBeGreaterThan(html.indexOf('<head>'));
	});
});
