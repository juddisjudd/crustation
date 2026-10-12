<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as Card from '$lib/components/ui/card/index.js';
	import { serverNetwork, type PortReport, type ServerNetwork } from '$lib/api/servers';
	import type { Server } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { t } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	type Tone = 'good' | 'bad' | 'unsure' | 'plain';

	const PRIVATE = /^(10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.)\d+\.\d+/;
	const LOCAL_NAME = /^[^.]+$|\.(local|lan|home|internal|home\.arpa|localdomain)$/i;
	const DOTS: Record<Tone, string> = {
		good: 'bg-success',
		bad: 'bg-destructive',
		unsure: 'bg-warning',
		plain: 'bg-muted-foreground/40'
	};

	const status = $derived(servers.statusOf(server.id));

	let network = $state.raw<ServerNetwork | null>(null);
	let loading = $state(true);
	let failed = $state(false);
	let copied = $state<string | null>(null);
	let asked = 0;

	const localHost = $derived.by(() => {
		const bound = server.address.host;
		if (bound && bound !== '0.0.0.0' && bound !== '::') return bound;
		const here = location.hostname;
		if (PRIVATE.test(here)) return here;
		if (network?.local) return network.local;
		// A name the browser found on the local network leads to the game server as well.
		return here !== 'localhost' && LOCAL_NAME.test(here) ? here : null;
	});
	const local = $derived(localHost ? `${localHost}:${server.address.port}` : null);
	const reached = $derived(network?.public ? `${network.public}:${server.address.port}` : null);
	const ports = $derived(network?.ports ?? []);

	const hint = $derived.by(() => {
		if (!network?.running || !ports.length) return null;
		const address = network.public ?? '';
		const has = (outside: PortReport['outside']) => ports.some((port) => port.outside === outside);
		if (has('closed')) {
			return session.info?.panel.docker
				? t('server.network.hints.closedDocker', { address })
				: t('server.network.hints.closed', { address });
		}
		if (has('unanswered')) return t('server.network.hints.unanswered', { address });
		if (has('open')) return t('server.network.hints.open', { address });
		return network.public ? null : t('server.network.hints.untested');
	});

	function verdict(port: PortReport): { tone: Tone; label: string; title?: string } {
		if (port.outside === 'open') return { tone: 'good', label: t('server.network.states.open') };
		if (port.outside === 'closed') return { tone: 'bad', label: t('server.network.states.closed') };
		if (port.listening === false) return { tone: 'bad', label: t('server.network.states.quiet') };
		if (port.outside === 'unanswered') {
			return { tone: 'unsure', label: t('server.network.states.unanswered') };
		}
		if (port.listening === null)
			return { tone: 'plain', label: t('server.network.states.unknown') };
		return {
			tone: 'plain',
			label: t('server.network.states.listening'),
			title: network?.public ? t('server.network.untestable') : undefined
		};
	}

	async function look() {
		const mine = ++asked;
		loading = true;
		try {
			const found = await serverNetwork(server.id);
			if (mine !== asked) return;
			network = found;
			failed = false;
		} catch {
			if (mine === asked) failed = true;
		} finally {
			if (mine === asked) loading = false;
		}
	}

	$effect(() => {
		void server.id;
		void status;
		look();
	});

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
			copied = text;
			setTimeout(() => (copied = null), 1500);
		} catch {
			// clipboard unavailable
		}
	}
</script>

<Card.Root>
	<Card.Header>
		<Card.Title>{t('server.network.title')}</Card.Title>
		<Card.Action>
			<Button
				variant="ghost"
				size="icon-sm"
				disabled={loading}
				title={t('server.network.refresh')}
				aria-label={t('server.network.refresh')}
				onclick={look}
			>
				<RefreshCwIcon class={loading ? 'animate-spin' : undefined} />
			</Button>
		</Card.Action>
	</Card.Header>
	<Card.Content class="space-y-4">
		<dl class="grid gap-3 text-sm">
			<div>
				<dt class="text-muted-foreground">{t('server.network.local')}</dt>
				<dd class="mt-1">
					{#if local}
						{@render address(local)}
					{:else}
						{t('server.network.localUnknown')}
						<span class="block text-xs text-muted-foreground">
							{t('server.network.localHint')}
						</span>
					{/if}
				</dd>
			</div>
			<div>
				<dt class="text-muted-foreground">{t('server.network.public')}</dt>
				<dd class="mt-1">
					{#if reached}
						{@render address(reached)}
					{:else if network}
						{t('server.network.publicUnknown')}
					{:else}
						<span class="text-muted-foreground">
							{failed ? t('server.network.failed') : t('server.network.checking')}
						</span>
					{/if}
				</dd>
			</div>
		</dl>

		{#if ports.length}
			<div class="text-sm">
				<h3 class="text-muted-foreground">{t('server.network.ports')}</h3>
				<ul class="mt-1 divide-y">
					{#each ports as port (port.name)}
						{@const found = network?.running ? verdict(port) : null}
						<li class="flex items-center gap-2 py-1.5">
							<span
								class={['size-1.5 shrink-0 rounded-full', DOTS[found?.tone ?? 'plain']]}
								aria-hidden="true"
							></span>
							<span>{t(`server.network.names.${port.name}`)}</span>
							<span class="font-mono text-xs text-muted-foreground uppercase">
								{port.port}
								{port.protocol}
							</span>
							{#if found}
								<span
									class={[
										'ml-auto',
										found.tone === 'bad' && 'text-destructive',
										found.tone === 'plain' && 'text-muted-foreground'
									]}
									title={found.title}
								>
									{found.label}
								</span>
							{/if}
						</li>
					{/each}
				</ul>
				{#if !network?.running}
					<p class="mt-2 text-xs text-muted-foreground">{t('server.network.whenRunning')}</p>
				{:else if hint}
					<p class="mt-2 text-xs text-muted-foreground">{hint}</p>
				{/if}
			</div>
		{/if}
	</Card.Content>
</Card.Root>

{#snippet address(text: string)}
	<button
		type="button"
		class="inline-flex cursor-pointer items-center gap-1.5 font-mono text-xs hover:text-foreground"
		title={t('common.actions.copy')}
		onclick={() => copy(text)}
	>
		{text}
		{#if copied === text}<CheckIcon class="size-3" />{:else}<CopyIcon class="size-3" />{/if}
	</button>
{/snippet}
