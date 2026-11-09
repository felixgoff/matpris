// Matpris as a desktop app for macOS, Windows and Linux (Electrobun 2). The SvelteKit server (`vite build`,
// adapter-node output in build/) is copied into the app and run by the Bun main
// process in desktop/main.ts. Build with `bun run desktop` / `bun run desktop:build`.
import type { ElectrobunConfig } from 'electrobun';
import { readFileSync } from 'node:fs';

// The packaged app has no .env, so the MapKit token is built in. It is a public
// token (every browser gets it), but it expires: rebuild after renewing it.
// Relative to this file: Hutch doesn't load the config from the project folder.
const env = (() => {
	try {
		return readFileSync(new URL('./.env', import.meta.url), 'utf8');
	} catch {
		return '';
	}
})();
const mapkitToken = /^PUBLIC_MAPKIT_TOKEN=(.*)$/m.exec(env)?.[1]?.trim().replace(/^(['"])(.*)\1$/, '$2') ?? '';
// One version for everything: `bun run bump patch` changes package.json, and a release
// is published for each version (.github/workflows/desktop-release.yml).
const { version } = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));

export default {
	app: {
		name: 'Matpris',
		identifier: 'se.matpris.desktop',
		version,
		description: 'Grocery prices from the stores around you'
	},
	// The updater reads the latest GitHub release. The repo is private, so desktop/update.ts
	// answers these URLs through the GitHub API with this Mac's `gh` login.
	release: { baseUrl: 'https://github.com/felixgoff/matpris/releases/latest/download', generatePatch: false },
	build: {
		mainProcess: 'bun',
		bun: {
			entrypoint: 'desktop/main.ts',
			define: { MAPKIT_TOKEN: JSON.stringify(mapkitToken) }
		},
		// Electrobun's own output must not share SvelteKit's build/.
		buildFolder: 'desktop/build',
		artifactFolder: 'desktop/artifacts',
		copy: { build: 'server' },
		// Icon Composer document; the other candidates are in desktop/icons/drafts.
		mac: { createDmg: false, icons: 'desktop/icons/Matpris.icon' },
		// The same icon flattened (desktop/icons/matpris.svg). Hutch builds for the host it
		// runs on only, so each OS has its own runner in desktop-release.yml.
		// "Use my location" needs WebView2's geolocation permission, granted to the app's
		// own localhost page (the only origin the window opens).
		win: { icon: 'desktop/icons/matpris.png', autoGrantPermissions: ['geolocation'] },
		// WebKitGTK 4.1, like Safari's engine on the Mac.
		linux: { icon: 'desktop/icons/matpris.png' }
	}
} satisfies ElectrobunConfig;
