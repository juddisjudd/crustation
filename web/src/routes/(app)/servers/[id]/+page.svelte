<script lang="ts">
	import * as Card from '$lib/components/ui/card/index.js';
	import Meter from '$lib/components/meter.svelte';
	import { servers, statusLabel } from '$lib/servers.svelte';
	import { bytes, dateTime, duration, kindLabel, parseTime, percent, stripMotd } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);
	const metrics = $derived(servers.metrics(server.id));
	const status = $derived(servers.statusOf(server.id));
	const startedAt = $derived(parseTime(metrics.started));

	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});
</script>

<svelte:head><title>{server.name} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-6xl space-y-6 px-4 py-8 md:px-8">
	<section
		aria-label={t('server.overview.liveUsage')}
		class="grid grid-cols-2 overflow-hidden rounded-lg border bg-border lg:grid-cols-4"
		style="gap: 1px"
	>
		<div class="flex flex-col gap-1 bg-background p-4">
			<span class="text-sm text-muted-foreground">{t('server.overview.uptime')}</span>
			<span class="text-2xl font-semibold tracking-tight tabular-nums">
				{startedAt ? duration(now - startedAt.getTime()) : '—'}
			</span>
			<span class="mt-auto text-xs text-muted-foreground">
				{startedAt
					? t('server.overview.startedAt', { when: dateTime(startedAt) })
					: statusLabel(status)}
			</span>
		</div>
		<div class="flex flex-col gap-1 bg-background p-4">
			<span class="text-sm text-muted-foreground">{t('dashboard.cpu')}</span>
			<span class="text-2xl font-semibold tracking-tight tabular-nums">
				{metrics.running ? percent(metrics.cpu, 1) : '—'}
			</span>
			<Meter value={metrics.cpu} label={t('server.overview.cpuUsage')} class="mt-auto" />
		</div>
		<div class="flex flex-col gap-1 bg-background p-4">
			<span class="text-sm text-muted-foreground">{t('dashboard.memory')}</span>
			<span class="text-2xl font-semibold tracking-tight tabular-nums">
				{metrics.running ? bytes(metrics.memoryBytes) : '—'}
			</span>
			<Meter value={metrics.memPercent} label={t('server.overview.memoryUsage')} class="mt-auto" />
		</div>
		<div class="flex flex-col gap-1 bg-background p-4">
			<span class="text-sm text-muted-foreground">{t('dashboard.players')}</span>
			<span class="text-2xl font-semibold tracking-tight tabular-nums">
				{metrics.running ? metrics.online : '—'}
				{#if metrics.running && metrics.max}
					<span class="text-base font-normal text-muted-foreground">/ {metrics.max}</span>
				{/if}
			</span>
			<Meter
				value={metrics.max ? (metrics.online / metrics.max) * 100 : 0}
				label={t('server.overview.slotsUsed')}
				tone="neutral"
				class="mt-auto"
			/>
		</div>
	</section>

	<Card.Root>
		<Card.Header>
			<Card.Title>{t('server.overview.details')}</Card.Title>
		</Card.Header>
		<Card.Content>
			<dl class="grid gap-x-6 gap-y-4 text-sm sm:grid-cols-2">
				<div>
					<dt class="text-muted-foreground">{t('server.overview.motd')}</dt>
					<dd class="mt-1">{stripMotd(metrics.motd) || '—'}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.overview.version')}</dt>
					<dd class="mt-1">{metrics.version || '—'}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.overview.type')}</dt>
					<dd class="mt-1">{kindLabel(server.kind)}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.overview.port')}</dt>
					<dd class="mt-1 font-mono text-xs">{server.address.port}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.overview.autoStart')}</dt>
					<dd class="mt-1">
						{server.settings.autostart
							? t('server.overview.autoStartOn', { seconds: server.settings.autostart_delay })
							: t('common.state.no')}
					</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.overview.memoryLimit')}</dt>
					<dd class="mt-1 tabular-nums">
						{server.settings.java.min_memory_mb} – {server.settings.java.max_memory_mb} MB
					</dd>
				</div>
				<div class="sm:col-span-2">
					<dt class="text-muted-foreground">{t('server.overview.folder')}</dt>
					<dd class="mt-1 truncate font-mono text-xs" title={server.settings.directory}>
						{server.settings.directory}
					</dd>
				</div>
				<div class="sm:col-span-2">
					<dt class="text-muted-foreground">{t('server.overview.command')}</dt>
					<dd class="mt-1 truncate font-mono text-xs" title={server.settings.command}>
						{server.settings.command || '—'}
					</dd>
				</div>
			</dl>
		</Card.Content>
	</Card.Root>
</div>
