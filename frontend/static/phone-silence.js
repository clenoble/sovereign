// On the phone, the page writes nothing to the system log (Céline, 2026-11-20).
// On Android the web view passes every console message and every uncaught
// error to the system log. This is the page's first code, a classic script
// loaded before any module: on a phone, recognised by its user agent, every
// console method does nothing, and uncaught errors and unhandled rejections are
// not reported. The page's own listeners still hear them. Anywhere else it does
// nothing.
(function () {
	var ua = (navigator && navigator.userAgent) || '';
	if (!/Android|iPhone|iPad|iPod/i.test(ua)) return;

	var quiet = function () {};
	var names = ['log', 'info', 'warn', 'error', 'debug', 'trace', 'dir', 'dirxml', 'table',
		'group', 'groupCollapsed', 'groupEnd', 'count', 'countReset', 'assert', 'time', 'timeLog',
		'timeEnd', 'timeStamp', 'profile', 'profileEnd', 'clear'];
	for (var k in console) {
		if (typeof console[k] === 'function' && names.indexOf(k) < 0) names.push(k);
	}
	for (var i = 0; i < names.length; i++) {
		if (typeof console[names[i]] === 'function') {
			try {
				console[names[i]] = quiet;
			} catch (e) {
				// a method that cannot be replaced stays as it is
			}
		}
	}

	var unreported = function (e) {
		e.preventDefault();
	};
	addEventListener('error', unreported);
	addEventListener('unhandledrejection', unreported);
})();
