<script lang="ts">
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ServerIcon from '@lucide/svelte/icons/server';
	import { resolve } from '$app/paths';
	import * as InputGroup from '$lib/components/ui/input-group/index.js';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import Meter from '$lib/components/meter.svelte';
	import StatusDot from '$lib/components/shell/status-dot.svelte';
	import ServerActionsMenu from '$lib/components/servers/server-actions-menu.svelte';
	import { servers, statusLabel } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { host } from '$lib/host.svelte';
	import { bytes, percent, kindLabel } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';

	let query = $state('');

	const totals = $derived(servers.totals);
	const filtered = $derived(
		servers.list.filter((server) => server.name.toLowerCase().includes(query.trim().toLowerCase()))
	);

	$effect(() => {
		host.load().catch(() => {});
		return host.listen();
	});
</script>

<svelte:head><title>{t('dashboard.title')} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-6xl px-4 py-8 md:px-8">
	<div class="flex flex-wrap items-end justify-between gap-4">
		<div>
			<h1 class="text-2xl font-semibold tracking-tight">{t('dashboard.title')}</h1>
			<p class="mt-1 text-sm text-muted-foreground">
				{#if servers.loaded}
					{t('dashboard.runningCount', { running: totals.running, total: totals.total })}
				{:else}
					{t('dashboard.loading')}
				{/if}
			</p>
		</div>
		{#if session.can('CREATE_SERVER')}
			<Button href={resolve('/servers/new')}>
				<PlusIcon />
				{t('nav.newServer')}
			</Button>
		{/if}
	</div>

	<section
		aria-label={t('dashboard.hostUsage')}
		class="mt-8 grid grid-cols-2 overflow-hidden rounded-lg border bg-border lg:grid-cols-4"
		style="gap: 1px"
	>
		{@render tile(
			t('dashboard.hostCpu'),
			percent(host.stats?.cpu.percent),
			host.stats?.cpu.percent,
			host.stats ? t('dashboard.cores', { cores: host.stats.cpu.cores }) : '',
			'usage',
			!!host.stats
		)}
		{@render tile(
			t('dashboard.hostMemory'),
			percent(host.stats?.memory.used_percent),
			host.stats?.memory.used_percent,
			host.stats
				? t('dashboard.memoryOf', {
						used: bytes(host.stats.memory.used_bytes),
						total: bytes(host.stats.memory.total_bytes)
					})
				: '',
			'usage',
			!!host.stats
		)}
		{@render tile(
			t('dashboard.servers'),
			`${totals.running}/${totals.total}`,
			totals.total ? (totals.running / totals.total) * 100 : 0,
			t('dashboard.stoppedCount', { count: totals.stopped }),
			'neutral'
		)}
		{@render tile(
			t('dashboard.players'),
			String(totals.players),
			null,
			totals.maxPlayers
				? t('dashboard.slotsOf', { max: totals.maxPlayers })
				: t('dashboard.acrossRunning')
		)}
	</section>

	{#if host.stats?.disks.length}
		<section
			aria-label={t('dashboard.storage')}
			class="mt-4 grid gap-3 rounded-lg border p-4 sm:grid-cols-2"
		>
			{#each host.stats.disks as disk (disk.mount)}
				<div class="space-y-2">
					<div class="flex items-baseline justify-between gap-2 text-sm">
						<span class="truncate font-mono text-xs">{disk.mount}</span>
						<span class="text-xs text-muted-foreground tabular-nums">
							{t('dashboard.memoryOf', {
								used: bytes(disk.used_bytes),
								total: bytes(disk.total_bytes)
							})}
						</span>
					</div>
					<Meter
						value={disk.used_percent}
						label={t('dashboard.diskUsage', { mount: disk.mount })}
					/>
				</div>
			{/each}
		</section>
	{/if}

	<section class="mt-10" aria-labelledby="servers-heading">
		<div class="mb-4 flex items-center gap-2">
			<h2 id="servers-heading" class="sr-only">{t('dashboard.servers')}</h2>
			<InputGroup.Root class="max-w-sm">
				<InputGroup.Addon><SearchIcon /></InputGroup.Addon>
				<InputGroup.Input
					placeholder={t('dashboard.searchServers')}
					bind:value={query}
					aria-label={t('dashboard.searchServers')}
				/>
			</InputGroup.Root>
		</div>

		{#if !servers.loaded}
			<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each [0, 1, 2] as i (i)}
					<Skeleton class="h-40 rounded-lg" />
				{/each}
			</div>
		{:else if servers.list.length === 0}
			<Empty.Root class="rounded-lg border border-dashed py-16">
				<Empty.Header>
					<Empty.Media variant="icon"><ServerIcon /></Empty.Media>
					<Empty.Title>{t('dashboard.emptyTitle')}</Empty.Title>
					<Empty.Description>{t('dashboard.emptyBody')}</Empty.Description>
				</Empty.Header>
				{#if session.can('CREATE_SERVER')}
					<Empty.Content>
						<Button href={resolve('/servers/new')}>
							<PlusIcon />
							{t('nav.newServer')}
						</Button>
					</Empty.Content>
				{/if}
			</Empty.Root>
		{:else if filtered.length === 0}
			<p class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
				{t('dashboard.noMatch', { query })}
			</p>
		{:else}
			<ul class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
				{#each filtered as server (server.id)}
					{@const metrics = servers.metrics(server.id)}
					{@const status = servers.statusOf(server.id)}
					{@const href = resolve(`/servers/${server.id}`)}
					<li
						class="group relative flex flex-col rounded-lg border bg-card p-4 transition-[border-color,box-shadow] hover:border-foreground/20 hover:shadow-xs"
					>
						<div class="flex items-start gap-3">
							<div class="min-w-0 flex-1">
								<a
									{href}
									class="block truncate font-medium outline-none after:absolute after:inset-0 after:rounded-lg focus-visible:after:ring-2 focus-visible:after:ring-ring"
								>
									{server.name}
								</a>
								<p class="truncate font-mono text-xs text-muted-foreground">
									{server.address.host}:{server.address.port}
								</p>
							</div>
							<div class="relative z-10 -mt-1 -mr-1"><ServerActionsMenu {server} /></div>
						</div>
						<div class="mt-4 flex items-center gap-2 text-xs">
							<span class="inline-flex items-center gap-1.5 font-medium">
								<StatusDot {status} />
								{statusLabel(status)}
							</span>
							<span class="truncate text-muted-foreground">{kindLabel(server.kind)}</span>
						</div>
						<dl class="mt-4 grid grid-cols-3 gap-3 border-t pt-4 text-xs">
							<div>
								<dt class="text-muted-foreground">{t('dashboard.cpu')}</dt>
								<dd class="mt-0.5 font-medium tabular-nums">
									{metrics.running ? percent(metrics.cpu, 1) : '—'}
								</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('dashboard.memory')}</dt>
								<dd class="mt-0.5 font-medium tabular-nums">
									{metrics.running ? bytes(metrics.memoryBytes) : '—'}
								</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('dashboard.players')}</dt>
								<dd class="mt-0.5 font-medium tabular-nums">
									{metrics.running ? `${metrics.online}/${metrics.max}` : '—'}
								</dd>
							</div>
						</dl>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

{#snippet tile(
	label: string,
	value: string,
	meter: number | null | undefined,
	hint: string,
	tone: 'usage' | 'neutral' = 'usage',
	ready = true
)}
	<div class="flex flex-col gap-1 bg-background p-4">
		<span class="text-sm text-muted-foreground">{label}</span>
		<span class="text-2xl font-semibold tracking-tight tabular-nums">
			{#if ready}{value}{:else}<Skeleton class="h-8 w-16" />{/if}
		</span>
		{#if meter !== null && meter !== undefined}
			<Meter value={meter} {label} {tone} class="mt-2" />
		{/if}
		<span class="mt-1 truncate text-xs text-muted-foreground">{hint}&nbsp;</span>
	</div>
{/snippet}
