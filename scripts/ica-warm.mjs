// Refreshes ICA's bot-protection session (aws-waf-token + shop cookies) into
// .ica-profile/session.json, so plain server-side HTTP can call the shop's own
// JSON APIs. Same trick as the ICA spider in matval: a small Playwright run keeps
// re-minting the short-lived token, and the cookies ride on every HTTP request.
//
// Run: bun run warm:ica            headless; exits once the token is minted
//      bun run warm:ica -- --keep  keeps the session fresh (re-mint every 3 min)
//                                  until Ctrl+C — for long sessions with the app
//      HEADED=1 bun run warm:ica   visible window, for when the WAF shows its
//                                  visual challenge: solve it by hand, exactly
//                                  as the crawler does
import { writeFile } from 'node:fs/promises';
import { chromium } from 'playwright';

const accountId = process.argv[2] && /^\d+$/.test(process.argv[2]) ? process.argv[2] : '1088004'; // Maxi ICA Stormarknad Gunnesbo, Lund
const KEEP = process.argv.includes('--keep');
const BASE = 'https://handlaprivatkund.ica.se';
const SESSION = new URL('./.ica-profile/session.json', import.meta.url).pathname;
const HEADLESS = process.env.HEADED !== '1';
const WAIT_SECONDS = Number(process.env.WAIT_SECONDS ?? (HEADLESS ? 30 : 600));

const CHALLENGE_MARKERS = ['Bekräfta att du är en människa', "Let's confirm you are human"];
const log = (...args) => console.log(new Date().toLocaleTimeString('sv-SE'), ...args);

const mint = async () => {
	const ctx = await chromium.launchPersistentContext(new URL('./.ica-profile/', import.meta.url).pathname, {
		headless: HEADLESS,
		channel: 'chrome',
		args: ['--disable-blink-features=AutomationControlled'],
		userAgent:
			'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36',
		locale: 'sv-SE',
		viewport: { width: 1200, height: 800 }
	});

	try {
		const page = ctx.pages()[0] ?? (await ctx.newPage());
		await page.goto(`${BASE}/stores/${accountId}`, { waitUntil: 'domcontentloaded', timeout: 60_000 });
		log(`warming (${HEADLESS ? 'headless' : 'headed — click any puzzle ICA shows'})…`);

		const hasToken = async () => (await ctx.cookies()).some((c) => c.name === 'aws-waf-token');
		const isChallenge = async () => {
			const content = await page.content().catch(() => '');
			return CHALLENGE_MARKERS.some((m) => content.includes(m));
		};

		// The passive challenge mints its token a few seconds after page load. A visual
		// challenge can only be cleared by a human in a headed window; we wait (and
		// beep) rather than automate anything past it.
		let token = false;
		for (let i = 0; i < WAIT_SECONDS; i++) {
			await page.waitForTimeout(1_000);
			if (await isChallenge()) {
				if (HEADLESS) throw new Error('the WAF shows a visual challenge — re-run headed and solve it by hand');
				if (i++ % 5 === 0) process.stdout.write('\x07');
				continue;
			}
			if (await hasToken()) {
				token = true;
				break;
			}
		}
		if (!token) throw new Error(`no WAF token minted within ${WAIT_SECONDS}s`);

		const cookies = (await ctx.cookies())
			.filter((c) => /ica\.se|amazonaws/i.test(c.domain))
			.map((c) => ({ name: c.name, value: c.value, domain: c.domain, path: c.path }));

		await writeFile(SESSION, JSON.stringify({ storedAt: Date.now(), accountId, cookies }));
		log(
			`session saved: ${cookies.length} cookies ` +
				`(fresh token: ${!!cookies.find((c) => c.name === 'aws-waf-token')})`
		);
	} finally {
		await ctx.close();
	}
};

if (KEEP) {
	log('keeping the session fresh until interrupted (Ctrl+C to stop)');
	for (;;) {
		try {
			await mint();
		} catch (e) {
			console.error(`  warm failed: ${e.message?.split('\n')[0]}`);
		}
		await new Promise((r) => setTimeout(r, 3 * 60_000));
	}
} else {
	await mint();
}
