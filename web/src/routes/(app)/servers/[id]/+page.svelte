<script lang="ts">
	import CopyIcon from '@lucide/svelte/icons/copy';
	import CheckIcon from '@lucide/svelte/icons/check';
	import * as Card from '$lib/components/ui/card/index.js';
	import Meter from '$lib/components/meter.svelte';
	import Sparkline from '$lib/components/sparkline.svelte';
	import { recentStats } from '$lib/api/servers';
	import AddonCard from '$lib/components/addon-card.svelte';
	import MotdEditor from '$lib/components/motd-editor.svelte';
	import NetworkCard from '$lib/components/servers/network-card.svelte';
	import RustHealthCard from '$lib/components/servers/rust-health-card.svelte';
	import RustUpdatesCard from '$lib/components/servers/rust-updates-card.svelte';
	import { servers, statusLabel } from '$lib/servers.svelte';
	import {
		bytes,
		dateTime,
		duration,
		isMinecraft,
		kindLabel,
		parseTime,
		percent
	} from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import { resolve } from '$app/paths';

	let { data } = $props();

	const server = $derived(data.server);
	const metrics = $derived(servers.metrics(server.id));
	const status = $derived(servers.statusOf(server.id));
	const startedAt = $derived(parseTime(metrics.started));
	const edition = $derived(server.kind === 'minecraft_bedrock' ? 'bedrock' : 'java');
	const minecraft = $derived(isMinecraft(server.kind));
	const rust = $derived(server.kind === 'rust');
	const convars = $derived(server.settings.convars ?? {});
	const frameworkLabel = $derived(
		t(`server.overview.rust.frameworks.${server.settings.framework ?? 'none'}`)
	);
	const host = $derived(
		!server.address.host || server.address.host === '0.0.0.0'
			? location.hostname
			: server.address.host
	);
	const connectCommand = $derived(`client.connect ${host}:${server.address.port}`);
	let connectCopied = $state(false);

	async function copyConnect() {
		try {
			await navigator.clipboard.writeText(connectCommand);
			connectCopied = true;
			setTimeout(() => (connectCopied = false), 1500);
		} catch {
			// clipboard unavailable
		}
	}
	const canConfigure = $derived(server.permissions.includes('CONFIG'));

	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 1000);
		return () => clearInterval(timer);
	});

	/** The last hour of usage, as percentages, newest last. */
	let cpuLine = $state.raw<(number | null)[]>([]);
	let memoryLine = $state.raw<(number | null)[]>([]);

	$effect(() => {
		const id = server.id;
		let live = true;
		const read = async () => {
			try {
				const found = await recentStats(60, 60);
				if (!live) return;
				const line = found.series[id] ?? [];
				cpuLine = line.map((point) => point?.cpu ?? null);
				memoryLine = line.map((point) => point?.memory_percent ?? null);
			} catch {
				// No history is no worse than the card not being there.
			}
		};
		read();
		const timer = setInterval(read, 30_000);
		return () => {
			live = false;
			clearInterval(timer);
		};
	});
</script>

<svelte:head><title>{server.name} · Crustation</title></svelte:head>

<div class="page space-y-6 py-6">
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

	<div
		class="grid gap-6 lg:grid-cols-[minmax(0,1fr)_minmax(0,22rem)] lg:items-start 2xl:grid-cols-[minmax(0,1fr)_minmax(0,28rem)]"
	>
		<div class="space-y-6">
			<Card.Root>
				<Card.Header>
					<Card.Title>{t('server.overview.details')}</Card.Title>
				</Card.Header>
				<Card.Content>
					<dl class="grid gap-x-6 gap-y-4 text-sm sm:grid-cols-2 xl:grid-cols-3">
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
						{#if rust}
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.queryPort')}</dt>
								<dd class="mt-1 font-mono text-xs">{convars['server.queryport'] ?? '—'}</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.name')}</dt>
								<dd class="mt-1 truncate" title={convars['server.hostname']}>
									{convars['server.hostname'] ?? server.name}
								</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.branch')}</dt>
								<dd class="mt-1">{server.settings.provider_version ?? '—'}</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.map')}</dt>
								<dd class="mt-1">{convars['server.level'] ?? 'Procedural Map'}</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.seed')}</dt>
								<dd class="mt-1 tabular-nums">
									{convars['server.seed'] ?? t('server.overview.rust.seedRandom')}
								</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.mapSize')}</dt>
								<dd class="mt-1 tabular-nums">{convars['server.worldsize'] ?? '3000'} m</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.gameMode')}</dt>
								<dd class="mt-1">
									{convars['server.gamemode'] ?? t('server.overview.rust.standard')}
								</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.framework')}</dt>
								<dd class="mt-1">
									{#if server.permissions.includes('FILES')}
										<a
											href={resolve(`/servers/${server.id}/plugins`)}
											class="underline decoration-muted-foreground/40 underline-offset-4 hover:decoration-foreground"
										>
											{frameworkLabel}
										</a>
									{:else}
										{frameworkLabel}
									{/if}
								</dd>
							</div>
							<div>
								<dt class="text-muted-foreground">{t('server.overview.rust.rules')}</dt>
								<dd class="mt-1">
									{convars['server.pve'] === 'true'
										? t('server.overview.rust.pve')
										: t('server.overview.rust.pvp')}
								</dd>
							</div>
							{#if convars['server.tags']}
								<div>
									<dt class="text-muted-foreground">{t('server.overview.rust.tags')}</dt>
									<dd class="mt-1">{convars['server.tags']}</dd>
								</div>
							{/if}
						{/if}
						{#if server.kind === 'minecraft_java'}
							<div>
								<dt class="text-muted-foreground">{t('server.overview.memoryLimit')}</dt>
								<dd class="mt-1 tabular-nums">
									{server.settings.java.min_memory_mb} – {server.settings.java.max_memory_mb} MB
								</dd>
							</div>
						{/if}
						<div>
							<dt class="text-muted-foreground">{t('server.overview.autoStart')}</dt>
							<dd class="mt-1">
								{server.settings.autostart
									? t('server.overview.autoStartOn', { seconds: server.settings.autostart_delay })
									: t('common.state.no')}
							</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('server.overview.latency')}</dt>
							<dd class="mt-1 tabular-nums">
								{metrics.latencyMs === null ? '—' : `${metrics.latencyMs} ms`}
							</dd>
						</div>
						<div class="col-span-full">
							<dt class="text-muted-foreground">{t('server.overview.folder')}</dt>
							<dd class="mt-1 truncate font-mono text-xs" title={server.settings.directory}>
								{server.settings.directory}
							</dd>
						</div>
						<div class="col-span-full">
							<dt class="text-muted-foreground">{t('server.overview.command')}</dt>
							<dd class="mt-1 truncate font-mono text-xs" title={server.settings.command}>
								{server.settings.command || '—'}
							</dd>
						</div>
					</dl>
				</Card.Content>
			</Card.Root>

			<Card.Root>
				<Card.Header>
					<Card.Title>{t('server.overview.activity.title')}</Card.Title>
				</Card.Header>
				<Card.Content class="grid gap-6 sm:grid-cols-2">
					{@render history(
						t('dashboard.cpu'),
						cpuLine,
						metrics.running ? percent(metrics.cpu, 1) : '—'
					)}
					{@render history(
						t('dashboard.memory'),
						memoryLine,
						metrics.running ? bytes(metrics.memoryBytes) : '—'
					)}
				</Card.Content>
			</Card.Root>
		</div>

		<div class="space-y-6">
			{#if rust}
				<Card.Root>
					<Card.Header>
						<Card.Title>{t('server.overview.rust.connectTitle')}</Card.Title>
						<Card.Description>{t('server.overview.rust.connectHint')}</Card.Description>
					</Card.Header>
					<Card.Content>
						<button
							type="button"
							class="flex w-full items-center gap-2 rounded-md bg-muted px-3 py-2 text-left font-mono text-xs break-all hover:text-foreground"
							title={t('common.actions.copy')}
							onclick={copyConnect}
						>
							<span class="min-w-0 flex-1">{connectCommand}</span>
							{#if connectCopied}<CheckIcon class="size-3.5 shrink-0" />{:else}<CopyIcon
									class="size-3.5 shrink-0"
								/>{/if}
						</button>
					</Card.Content>
				</Card.Root>
			{/if}
			<NetworkCard {server} />
			{#if rust && server.permissions.includes('CONSOLE')}
				<RustHealthCard serverId={server.id} />
			{/if}
			{#if rust && canConfigure}
				<RustUpdatesCard {server} />
			{/if}
			{#if minecraft}
				<Card.Root>
					<Card.Header>
						<Card.Title>{t('server.motd.title')}</Card.Title>
					</Card.Header>
					<Card.Content>
						<MotdEditor serverId={server.id} {edition} canEdit={canConfigure} />
					</Card.Content>
				</Card.Root>
			{/if}

			{#if edition === 'bedrock'}
				<Card.Root>
					<Card.Header>
						<Card.Title>{t('addon.title')}</Card.Title>
						<Card.Description>{t('addon.hint')}</Card.Description>
					</Card.Header>
					<Card.Content>
						<AddonCard serverId={server.id} canEdit={canConfigure} />
					</Card.Content>
				</Card.Root>
			{/if}
		</div>
	</div>
</div>

{#snippet history(label: string, points: (number | null)[], value: string)}
	<div>
		<div class="flex items-baseline justify-between gap-2 text-sm">
			<span class="text-muted-foreground">{label}</span>
			<span class="font-medium tabular-nums">{value}</span>
		</div>
		{#if points.some((point) => point !== null)}
			<Sparkline
				{points}
				label={t('server.overview.activity.label', { name: label })}
				class="mt-2 h-20 w-full rounded-md bg-muted/40 text-foreground/80"
			/>
		{:else}
			<p
				class="mt-2 grid h-20 place-items-center rounded-md bg-muted/40 text-xs text-muted-foreground"
			>
				{t('server.overview.activity.noData')}
			</p>
		{/if}
	</div>
{/snippet}
