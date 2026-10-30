// Matpris as a macOS desktop app (Electrobun 2). The SvelteKit server (`vite build`,
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

export default {
	app: {
		name: 'Matpris',
		identifier: 'se.matpris.desktop',
		version: '0.0.1',
		description: 'Grocery prices from the stores around you'
	},
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
		mac: { createDmg: false, icons: 'desktop/icons/Matpris.icon' }
	}
} satisfies ElectrobunConfig;
