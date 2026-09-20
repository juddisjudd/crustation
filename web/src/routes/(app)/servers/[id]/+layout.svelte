<script lang="ts">
	import PlayIcon from '@lucide/svelte/icons/play';
	import SquareIcon from '@lucide/svelte/icons/square';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import CheckIcon from '@lucide/svelte/icons/check';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import { invalidate } from '$app/navigation';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import ServerActionsMenu from '$lib/components/servers/server-actions-menu.svelte';
	import StatusBadge from '$lib/components/servers/status-badge.svelte';
	import { powerAction, type PowerAction } from '$lib/api/servers';
	import { portClashes, servers } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { socket } from '$lib/realtime/socket.svelte';
	import { visibleTabs } from '$lib/server-tabs';
	import { kindLabel } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';

	let { data, children } = $props();

	const server = $derived(data.server);
	const metrics = $derived(servers.metrics(server.id));
	const status = $derived(servers.statusOf(server.id));
	const tabs = $derived(visibleTabs(server, session.superuser));
	const clashes = $derived(portClashes(servers.list, server.id));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));
	const busy = $derived(status === 'installing' || server.flags.updating);
	const address = $derived(
		`${!server.address.host || server.address.host === '0.0.0.0' ? location.hostname : server.address.host}:${server.address.port}`
	);

	let pending = $state<PowerAction | null>(null);
	let copied = $state(false);

	$effect(() => socket.subscribe([`server:${server.id}`]));

	$effect(() =>
		socket.on('servers', 'state', () => {
			invalidate('crustation:server');
		})
	);

	async function power(action: PowerAction) {
		pending = action;
		await powerAction(server.id, action, server.name);
		setTimeout(() => (pending = null), 1500);
	}

	async function copyAddress() {
		try {
			await navigator.clipboard.writeText(address);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			// clipboard unavailable
		}
	}

	function tabHref(slug: string) {
		const root = resolve(`/servers/${server.id}`);
		return slug ? `${root}/${slug}` : root;
	}

	function isActive(slug: string) {
		const href = tabHref(slug);
		const path = page.url.pathname.replace(/\/$/, '');
		return slug ? path === href || path.startsWith(href + '/') : path === href;
	}
</script>

<div class="border-b">
	<div class="mx-auto w-full max-w-6xl px-4 pt-6 md:px-8">
		<div class="flex flex-wrap items-start gap-4">
			<div class="min-w-0 flex-1">
				<div class="flex flex-wrap items-center gap-2">
					<h1 class="truncate text-2xl font-semibold tracking-tight">{server.name}</h1>
					<StatusBadge {status} />
				</div>
				<div class="mt-1 flex flex-wrap items-center gap-x-3 gap-y-1 text-sm text-muted-foreground">
					<button
						type="button"
						class="inline-flex items-center gap-1.5 font-mono text-xs hover:text-foreground"
						onclick={copyAddress}
						title={t('server.copyAddress')}
					>
						{address}
						{#if copied}<CheckIcon class="size-3" />{:else}<CopyIcon class="size-3" />{/if}
					</button>
					<span aria-hidden="true">·</span>
					<span>{kindLabel(server.kind)}</span>
					{#if metrics.version}
						<span aria-hidden="true">·</span>
						<span class="truncate">{metrics.version}</span>
					{/if}
				</div>
			</div>
			{#if canCommand}
				<div class="flex items-center gap-2">
					{#if busy}
						<Button variant="outline" disabled>
							<Spinner />
							{t('server.installing')}
						</Button>
					{:else if metrics.running}
						<Button variant="outline" disabled={!!pending} onclick={() => power('restart')}>
							{#if pending === 'restart'}<Spinner />{:else}<RotateCcwIcon />{/if}
							{t('common.server.action.restart')}
						</Button>
						<Button variant="outline" disabled={!!pending} onclick={() => power('stop')}>
							{#if pending === 'stop'}<Spinner />{:else}<SquareIcon />{/if}
							{t('common.server.action.stop')}
						</Button>
					{:else}
						<Button disabled={!!pending} onclick={() => power('start')}>
							{#if pending === 'start'}<Spinner />{:else}<PlayIcon />{/if}
							{t('common.server.action.start')}
						</Button>
					{/if}
					<ServerActionsMenu {server} />
				</div>
			{/if}
		</div>

		{#if clashes.length}
			<p class="mt-3 text-sm text-warning">
				{t('server.portClash', {
					port: server.address.port,
					names: clashes.map((other) => other.name).join(', ')
				})}
			</p>
		{/if}

		<nav
			aria-label={t('nav.servers')}
			class="mt-5 -mb-px flex [scrollbar-width:none] gap-1 overflow-x-auto"
		>
			{#each tabs as tab (tab.slug)}
				{@const active = isActive(tab.slug)}
				<a
					href={tabHref(tab.slug)}
					aria-current={active ? 'page' : undefined}
					class={[
						'relative shrink-0 rounded-md px-3 py-2 text-sm transition-colors',
						'after:absolute after:inset-x-2 after:-bottom-px after:h-0.5 after:rounded-full',
						active
							? 'text-foreground after:bg-foreground'
							: 'text-muted-foreground hover:bg-accent hover:text-foreground'
					]}
				>
					{t(tab.label)}
				</a>
			{/each}
		</nav>
	</div>
</div>

{@render children()}
