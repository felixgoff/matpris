// Self-update, like RustRadar: on launch a newer release is downloaded and installed
// before the window opens (a small progress window shows meanwhile); while the app is
// open it looks hourly, downloads quietly, and the page offers a restart. An update is
// never a reason not to start: offline, no login, or any failure opens the installed
// version.
//
// Releases are on this private repo, so the updater's plain requests to the release
// download URL (release.baseUrl in electrobun.config.ts) are answered through the
// GitHub API with this Mac's `gh` login. The token is asked for when needed and kept
// in memory only; without gh there is simply never an update.
import { BrowserWindow, Updater } from 'electrobun/main';
import { existsSync } from 'node:fs';
import type { IncomingMessage, ServerResponse } from 'node:http';

const DOWNLOADS = 'https://github.com/felixgoff/matpris/releases/latest/download/';
const LATEST = 'https://api.github.com/repos/felixgoff/matpris/releases/latest';
// Apps opened from Finder don't get the shell's PATH.
const GH = ['/opt/homebrew/bin/gh', '/usr/local/bin/gh'].find((p) => existsSync(p));
const CHECK_TIMEOUT_MS = 8_000;
const WATCH_EVERY_MS = 60 * 60 * 1000;

let token: string | undefined;
function ghToken() {
	if (token === undefined) {
		const out = GH ? Bun.spawnSync([GH, 'auth', 'token']) : undefined;
		token = out?.success ? out.stdout.toString().trim() : '';
	}
	return token || undefined;
}

const plainFetch = globalThis.fetch;
/** Electrobun's updater fetches `${baseUrl}/<file>` with no way to log in: serve those
 *  files from the latest release's assets instead. Everything else passes through. */
async function releaseFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
	const url = input instanceof Request ? input.url : String(input);
	if (!url.startsWith(DOWNLOADS)) return plainFetch(input, init);
	const auth = ghToken();
	if (!auth) return new Response('No GitHub login: run `gh auth login`', { status: 401 });
	const headers = { authorization: `Bearer ${auth}`, 'x-github-api-version': '2022-11-28' };
	const release = await plainFetch(LATEST, { headers, signal: init?.signal });
	if (!release.ok) return new Response(null, { status: release.status });
	const name = decodeURIComponent(new URL(url).pathname.split('/').pop() ?? '');
	const asset = ((await release.json()).assets as { name: string; url: string }[]).find((a) => a.name === name);
	if (!asset) return new Response(null, { status: 404 });
	// The asset answers with a redirect to signed storage, which must not get the token.
	const hop = await plainFetch(asset.url, {
		headers: { ...headers, accept: 'application/octet-stream' },
		redirect: 'manual',
		signal: init?.signal
	});
	const location = hop.headers.get('location');
	return location ? plainFetch(location, { signal: init?.signal }) : hop;
}
globalThis.fetch = Object.assign(releaseFetch, { preconnect: plainFetch.preconnect });

/** What the page and the progress window are told. */
const state: { phase: 'idle' | 'downloading' | 'ready' | 'installing'; version?: string; progress?: number } = {
	phase: 'idle'
};
Updater.onStatusChange((e) => {
	if (e.status === 'download-progress' && e.details?.progress != null) state.progress = e.details.progress;
});

const within = <T>(promise: Promise<T>, ms: number) =>
	Promise.race([promise, new Promise<never>((_, fail) => setTimeout(() => fail(new Error('timed out')), ms))]);

/** The newer release's version, if there is one this app can update to. */
async function newerRelease(): Promise<string | undefined> {
	// Dev builds have nothing published to update to.
	if ((await Updater.getLocalInfo()).channel === 'dev' || !ghToken()) return undefined;
	const info = await within(Updater.checkForUpdate(), CHECK_TIMEOUT_MS);
	return info.updateAvailable && !info.error ? info.version : undefined;
}

async function download(version: string) {
	Object.assign(state, { phase: 'downloading', version, progress: undefined });
	await Updater.downloadUpdate();
	const info = Updater.updateInfo();
	if (!info.updateReady) throw new Error(info.error || 'the download did not finish');
}

/** On launch: install a newer release before the window opens. Returns if there was
 *  none or it failed; on success the app quits and the new version starts. */
export async function updateBeforeLaunch(port: number, frame: { x: number; y: number; width: number; height: number }) {
	let progress: BrowserWindow | undefined;
	try {
		const version = await newerRelease();
		if (!version) return;
		progress = new BrowserWindow({
			title: 'Updating Matpris',
			url: `http://127.0.0.1:${port}/__desktop/updating`,
			titleBarStyle: 'hidden',
			frame: { width: 380, height: 150, x: frame.x + (frame.width - 380) / 2, y: frame.y + (frame.height - 150) / 2 }
		});
		await download(version);
		state.phase = 'installing';
		await Updater.applyUpdate();
	} catch (e) {
		console.error('update skipped:', (e as Error).message);
	} finally {
		progress?.close();
		state.phase = 'idle';
	}
}

/** While the app is open: look hourly and download quietly; the page offers a restart. */
export function watchForUpdates() {
	setInterval(async () => {
		if (state.phase !== 'idle') return;
		try {
			const version = await newerRelease();
			if (!version) return;
			await download(version);
			state.phase = 'ready';
		} catch (e) {
			state.phase = 'idle';
			console.error('update check failed:', (e as Error).message);
		}
	}, WATCH_EVERY_MS);
}

/** The app's own routes, ahead of SvelteKit: the update state for the page
 *  (GET /__desktop/update), a restart into a downloaded update (POST …/restart), and
 *  the launch progress window's page. Returns false for every other request. */
export function desktopRoute(req: IncomingMessage, res: ServerResponse): boolean {
	if (req.url === '/__desktop/update' && req.method === 'GET') {
		res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
		res.end(JSON.stringify(state));
	} else if (req.url === '/__desktop/update/restart' && req.method === 'POST') {
		if (state.phase !== 'ready') {
			res.writeHead(409).end('No update is waiting');
			return true;
		}
		state.phase = 'installing';
		res.writeHead(202).end();
		Updater.applyUpdate().catch((e: Error) => {
			state.phase = 'ready';
			console.error('update install failed:', e.message);
		});
	} else if (req.url === '/__desktop/updating') {
		res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
		res.end(PROGRESS_PAGE);
	} else return false;
	return true;
}

// The launch progress window: the Shelf Edge rail, the wordmark and a yellow bar.
const PROGRESS_PAGE = `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Updating Matpris</title>
<style>
@font-face{font-family:Archivo;src:url(/fonts/archivo-latin.woff2) format("woff2");font-weight:100 900;font-stretch:62% 125%}
html{background:#26292b;color:#fff;font:600 13px/1.4 Archivo,system-ui,sans-serif;height:100%}
body{margin:0;height:100%;display:grid;place-content:center;gap:12px;padding:0 32px;-webkit-user-select:none;cursor:default}
b{font:900 24px/1 Archivo;font-stretch:70%;letter-spacing:-.01em}b i{font-style:normal;color:#ffd400}
.bar{width:316px;height:6px;border-radius:2px;background:#3a3e41;overflow:hidden}
.bar span{display:block;height:100%;width:0;background:#ffd400;transition:width .2s}
.bar.busy span{width:35%;animation:slide 1.1s cubic-bezier(.16,1,.3,1) infinite}
@keyframes slide{from{transform:translateX(-100%)}to{transform:translateX(320%)}}
@media (prefers-reduced-motion:reduce){.bar.busy span{animation:none;width:100%;opacity:.5}}
</style>
<b>MAT<i>PRIS</i></b><p id="status">Updating Matpris…</p><div class="bar busy" id="bar"><span></span></div>
<script>
const status = document.getElementById('status'), bar = document.getElementById('bar');
setInterval(async () => {
	const s = await fetch('/__desktop/update').then((r) => r.json()).catch(() => null);
	if (!s) return;
	const known = s.phase === 'downloading' && s.progress != null;
	if (s.phase === 'installing') status.textContent = 'Installing…';
	else if (s.phase === 'downloading') status.textContent = 'Downloading Matpris ' + s.version + '…';
	bar.classList.toggle('busy', !known);
	bar.firstElementChild.style.width = known ? s.progress + '%' : '';
}, 250);
</script>`;
