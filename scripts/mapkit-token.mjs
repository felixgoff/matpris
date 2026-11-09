// Signs a MapKit JS token (an ES256 JWT) from the MapKit private key, so the map never
// depends on a hand-made token that quietly expires. No origin claim: the apps' pages
// come from tauri://localhost / http://tauri.localhost, not a website, so a token tied
// to a hostname would be refused there.
//
//   bun run mapkit:token              # writes PUBLIC_MAPKIT_TOKEN into .env
//   node scripts/mapkit-token.mjs -   # prints the token (the Release workflow uses this)
//
// The key: MAPKIT_PRIVATE_KEY (its PEM text, as in CI), or the file MAPKIT_KEY_FILE, or
// the first ~/.matpris/AuthKey_*.p8. Key ID: MAPKIT_KEY_ID, else read from the file
// name. Team: MAPKIT_TEAM_ID, default JWR5UX55S5. Lifetime: MAPKIT_TOKEN_DAYS (180).
import { createPrivateKey, sign } from 'node:crypto';
import { readFileSync, readdirSync, statSync, writeFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const fail = (message) => {
	console.error(message);
	process.exit(1);
};

let pem = process.env.MAPKIT_PRIVATE_KEY;
let keyId = process.env.MAPKIT_KEY_ID;
if (!pem) {
	let file = process.env.MAPKIT_KEY_FILE;
	if (!file) {
		const dir = join(homedir(), '.matpris');
		const found = existsSync(dir)
			? readdirSync(dir)
					.filter((f) => /^AuthKey_\w+\.p8$/.test(f))
					.sort((a, b) => statSync(join(dir, b)).mtimeMs - statSync(join(dir, a)).mtimeMs)[0] // newest
			: undefined;
		if (!found) fail('No MapKit key: put AuthKey_<KEYID>.p8 in ~/.matpris/, or set MAPKIT_KEY_FILE.');
		file = join(dir, found);
	}
	pem = readFileSync(file, 'utf8');
	keyId ||= /AuthKey_(\w+)\.p8$/.exec(file)?.[1];
}
if (!keyId) fail('No key ID: set MAPKIT_KEY_ID (the 10 characters in AuthKey_<KEYID>.p8).');
const teamId = process.env.MAPKIT_TEAM_ID || 'JWR5UX55S5';
const days = Number(process.env.MAPKIT_TOKEN_DAYS || 180);

const b64url = (data) => Buffer.from(data).toString('base64url');
const now = Math.floor(Date.now() / 1000);
const header = b64url(JSON.stringify({ alg: 'ES256', kid: keyId, typ: 'JWT' }));
const payload = b64url(JSON.stringify({ iss: teamId, iat: now, exp: now + days * 86400, scope: 'mapkit_js' }));
// JWTs want the raw r||s signature, not DER.
const signature = sign('sha256', Buffer.from(`${header}.${payload}`), {
	key: createPrivateKey(pem),
	dsaEncoding: 'ieee-p1363'
});
const token = `${header}.${payload}.${b64url(signature)}`;

if (process.argv[2] === '-') {
	process.stdout.write(token);
} else {
	const env = new URL('../.env', import.meta.url);
	const text = existsSync(env) ? readFileSync(env, 'utf8') : '';
	const line = `PUBLIC_MAPKIT_TOKEN=${token}`;
	const next = /^PUBLIC_MAPKIT_TOKEN=.*$/m.test(text)
		? text.replace(/^PUBLIC_MAPKIT_TOKEN=.*$/m, line)
		: `${text}${text && !text.endsWith('\n') ? '\n' : ''}${line}\n`;
	writeFileSync(env, next);
	const until = new Date((now + days * 86400) * 1000).toISOString().slice(0, 10);
	console.log(`.env: a new MapKit token (key ${keyId}), valid until ${until}. Restart bun run dev / rebuild the app.`);
}
