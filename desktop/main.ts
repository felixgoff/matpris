// Desktop main process: runs the SvelteKit server (copied into the app as server/,
// see electrobun.config.ts) on a free localhost port and opens a window on it.
// Everything the web app does still happens in that server, inside this process.
import { ApplicationMenu, BrowserWindow } from 'electrobun/main';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { desktopRoute, updateBeforeLaunch, watchForUpdates } from './update';

/** Built in from .env by electrobun.config.ts. */
declare const MAPKIT_TOKEN: string;

process.env.PUBLIC_MAPKIT_TOKEN ||= MAPKIT_TOKEN;

// On macOS, Cmd+A/C/V/X/Z are menu shortcuts: without an Edit menu they do nothing in
// the window's text fields.
ApplicationMenu.setApplicationMenu([
	{ label: 'Matpris', submenu: [{ role: 'hide' }, { role: 'hideOthers' }, { role: 'showAll' }, { type: 'separator' }, { role: 'quit' }] },
	{
		label: 'Edit',
		submenu: [
			{ role: 'undo' },
			{ role: 'redo' },
			{ type: 'separator' },
			{ role: 'cut' },
			{ role: 'copy' },
			{ role: 'paste' },
			{ role: 'selectAll' }
		]
	},
	{ label: 'Window', submenu: [{ role: 'minimize' }, { role: 'zoom' }, { role: 'close' }] }
]);

const serverDir = join(import.meta.dir, '..', 'server');
const { handler } = await import(pathToFileURL(join(serverDir, 'handler.js')).href);

const server = createServer((req, res) => desktopRoute(req, res) || handler(req, res));
await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
const { port } = server.address() as AddressInfo;
console.log(`Matpris server on http://127.0.0.1:${port}/`);

const frame = { x: 120, y: 80, width: 1280, height: 820 };
// A newer release installs before the window opens; then the app restarts as it.
await updateBeforeLaunch(port, frame);
new BrowserWindow({ title: 'Matpris', url: `http://127.0.0.1:${port}/`, frame });
watchForUpdates();
