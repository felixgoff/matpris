<script lang="ts">
	import { onMount } from 'svelte';

	// Desktop app only (desktop/update.ts): a newer version downloaded while the app was
	// open waits for a restart. Opening the app installs it anyway, so Later only hides
	// the offer for now.
	let version = $state<string>();
	let dismissed = $state<string>();
	let restarting = $state(false);
	let error = $state('');

	onMount(() => {
		// Electrobun's preload marks its windows; a browser tab never asks.
		if (!('__electrobunWebviewId' in window)) return;
		const poll = async () => {
			const s = await fetch('/__desktop/update')
				.then((r) => r.json())
				.catch(() => undefined);
			version = s?.phase === 'ready' || s?.phase === 'installing' ? s.version : undefined;
		};
		poll();
		const timer = setInterval(poll, 5 * 60 * 1000);
		return () => clearInterval(timer);
	});

	async function restart() {
		restarting = true;
		error = '';
		const res = await fetch('/__desktop/update/restart', { method: 'POST' }).catch(() => undefined);
		if (!res?.ok) {
			restarting = false;
			error = "Couldn't install it now. It installs the next time Matpris opens.";
		}
	}
</script>

{#if version && dismissed !== version}
	<div class="toast" role="status" aria-live="polite">
		<div class="text">
			<p class="title">Matpris {version} is ready</p>
			<p class="sub">{error || 'Restart to start using it. It takes a few seconds.'}</p>
		</div>
		<button class="restart" onclick={restart} disabled={restarting}>{restarting ? 'Restarting…' : 'Restart'}</button>
		<button class="later" onclick={() => (dismissed = version)} title="It installs the next time Matpris opens">
			Later
		</button>
	</div>
{/if}

<style>
	/* A label hanging from its steel strip, like the results. */
	.toast {
		position: fixed;
		left: 16px;
		bottom: 16px;
		z-index: 30;
		display: flex;
		align-items: center;
		gap: 10px;
		max-width: min(460px, calc(100vw - 32px));
		padding: 12px 12px 12px 16px;
		background: var(--label);
		border-radius: 0 0 3px 3px;
		box-shadow: 0 8px 18px rgb(0 0 0 / 0.18);
	}
	.toast::before {
		content: '';
		position: absolute;
		left: 0;
		right: 0;
		top: -12px;
		height: 12px;
		border-radius: 2px 2px 0 0;
		background: linear-gradient(#61666a, var(--steel-edge));
	}
	.text {
		min-width: 0;
		margin-right: 6px;
	}
	.title {
		font-weight: 700;
	}
	.sub {
		font-size: 0.8125rem;
		color: var(--ink-2);
	}
	button {
		flex-shrink: 0;
		padding: 7px 12px;
		border: 1.5px solid var(--steel);
		border-radius: 3px;
		font-size: 0.8125rem;
		font-weight: 600;
		cursor: pointer;
	}
	.restart {
		background: var(--steel);
		color: #fff;
	}
	.later {
		background: var(--label);
		color: var(--ink);
	}
	.later:hover {
		background: var(--ground);
	}
	button:disabled {
		opacity: 0.7;
		cursor: default;
	}
</style>
