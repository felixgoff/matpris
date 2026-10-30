/// <reference types="vitest/config" />
import adapter from '@sveltejs/adapter-node';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	// Only our tests: Electrobun's SDK copy in .hutch/ ships its own.
	test: { include: ['src/**/*.test.ts'] },
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			// A standalone server in build/: run it with `node build`, or inside the desktop
			// app (desktop/main.ts), which starts its handler on a free localhost port.
			adapter: adapter()
		})
	]
});
