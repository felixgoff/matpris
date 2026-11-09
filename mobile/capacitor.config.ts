// Matpris for Android and iPhone. The app is a native window on the Matpris server
// (the same SvelteKit app as the web and desktop), so the phone gets every feature
// as soon as the server has it, with no app update. Phones can't run the server
// themselves: host it (Dockerfile in the repo root) and build with its address:
//   MATPRIS_SERVER_URL=https://matpris.example.se bun run sync
// Product links and other sites open in the system browser; only the server's pages
// load in the app. "Use my location" is the page's own request: Capacitor turns it into
// the system's location prompt (permissions in AndroidManifest.xml and Info.plist).
import type { CapacitorConfig } from '@capacitor/cli';

const server = process.env.MATPRIS_SERVER_URL?.trim().replace(/\/+$/, '');

const config: CapacitorConfig = {
	appId: 'se.matpris.app',
	appName: 'Matpris',
	// Local pages: the "no server" notice, and the offline page (errorPath).
	webDir: 'www',
	backgroundColor: '#26292b',
	server: server
		? {
				url: server,
				// Plain http only for a dev server on the computer (emulator: http://10.0.2.2:5173).
				cleartext: server.startsWith('http://'),
				errorPath: 'offline.html'
			}
		: undefined,
	ios: {
		// Keep the page clear of the notch and the home indicator.
		contentInset: 'automatic'
	},
	plugins: {
		// The page doesn't draw under the system bars: Android pads the WebView instead.
		SystemBars: { insetsHandling: 'native' }
	}
};

export default config;
