import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

// Tauri sets TAURI_DEV_HOST when the page is served to a phone during `tauri android|ios dev`.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},

			// A static page in build/: the Tauri app ships it, and its Rust backend answers
			// the page through commands (src/lib/api.ts). In a browser (`bun run dev`) the
			// page asks the Rust dev server instead, through the /api proxy below.
			adapter: adapter({ fallback: 'index.html' })
		})
	],
	// Tauri expects a fixed port and shows its own errors.
	clearScreen: false,
	server: {
		port: 5173,
		strictPort: true,
		host: host || false,
		hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
		watch: { ignored: ['**/src-tauri/**', '**/target/**', '**/core/**', '**/server/**', '**/wear/**'] },
		// The Rust dev server (server/, `bun run server`).
		proxy: { '/api': 'http://127.0.0.1:8787' }
	}
});
