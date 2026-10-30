// Desktop main process: runs the SvelteKit server (copied into the app as server/,
// see electrobun.config.ts) on a free localhost port and opens a window on it.
// Everything the web app does still happens in that server, inside this process.
import { BrowserWindow } from 'electrobun/main';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

/** Built in from .env by electrobun.config.ts. */
declare const MAPKIT_TOKEN: string;

process.env.PUBLIC_MAPKIT_TOKEN ||= MAPKIT_TOKEN;

const serverDir = join(import.meta.dir, '..', 'server');
const { handler } = await import(pathToFileURL(join(serverDir, 'handler.js')).href);

const server = createServer(handler);
await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve));
const { port } = server.address() as AddressInfo;
console.log(`Matpris server on http://127.0.0.1:${port}/`);

new BrowserWindow({
	title: 'Matpris',
	url: `http://127.0.0.1:${port}/`,
	frame: { x: 120, y: 80, width: 1280, height: 820 }
});
