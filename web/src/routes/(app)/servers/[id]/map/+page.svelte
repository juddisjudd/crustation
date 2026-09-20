<script lang="ts">
	import ExternalLinkIcon from '@lucide/svelte/icons/external-link';
	import RefreshIcon from '@lucide/svelte/icons/refresh-cw';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import { headUrl } from '$lib/api/players';
	import { positions, webMap, type Positions, type WebMap } from '$lib/api/map';
	import { servers } from '$lib/servers.svelte';
	import { t } from '$lib/i18n/index.svelte';

	const EVERY_MS = 3000;
	/** Never zoom in past this, so one player alone is not a pointless close-up. */
	const LEAST_SPAN = 256;

	let { data } = $props();

	const server = $derived(data.server);
	const running = $derived(servers.isRunning(server.id));

	let installed = $state.raw<WebMap | null>(null);
	let live = $state.raw<Positions | null>(null);
	let dimension = $state('minecraft:overworld');
	let refreshing = $state(false);

	/** The map plugin serves itself, on the same host as the panel. */
	const mapUrl = $derived.by(() => {
		if (!installed?.found) return null;
		const where = typeof location === 'undefined' ? null : location;
		if (!where) return null;
		return `${where.protocol}//${where.hostname}:${installed.port}/`;
	});

	$effect(() => {
		const id = server.id;
		installed = null;
		webMap(id)
			.then((found) => {
				if (server.id === id) installed = found;
			})
			.catch(() => {
				if (server.id === id) installed = { found: false };
			});
	});

	// Only poll the built-in view, and only while there is something to ask.
	$effect(() => {
		const id = server.id;
		const embedded = installed?.found && installed.answering;
		if (embedded || !installed) return;

		let stopped = false;
		let timer: ReturnType<typeof setInterval> | undefined;
		const tick = async () => {
			try {
				const found = await positions(id);
				if (stopped || server.id !== id) return;
				live = found;
				// Nothing will change by asking again, and on a server without
				// RCON each ask puts another `list` down its stdin.
				if (!found.supported) clearInterval(timer);
			} catch {
				if (!stopped) live = null;
			}
		};
		tick();
		timer = setInterval(tick, EVERY_MS);
		return () => {
			stopped = true;
			clearInterval(timer);
		};
	});

	const dimensions = $derived([...new Set((live?.players ?? []).map((one) => one.dimension))]);
	const shown = $derived((live?.players ?? []).filter((one) => one.dimension === dimension));

	/** A square window around whoever is on, never tighter than LEAST_SPAN. */
	const view = $derived.by(() => {
		if (!shown.length) return { x: -LEAST_SPAN / 2, z: -LEAST_SPAN / 2, span: LEAST_SPAN };
		const xs = shown.map((one) => one.x);
		const zs = shown.map((one) => one.z);
		const midX = (Math.min(...xs) + Math.max(...xs)) / 2;
		const midZ = (Math.min(...zs) + Math.max(...zs)) / 2;
		const span = Math.max(
			LEAST_SPAN,
			(Math.max(...xs) - Math.min(...xs)) * 1.4,
			(Math.max(...zs) - Math.min(...zs)) * 1.4
		);
		return { x: midX - span / 2, z: midZ - span / 2, span };
	});

	const at = (one: { x: number; z: number }) => ({
		left: `${((one.x - view.x) / view.span) * 100}%`,
		// North is −Z, so the smaller Z belongs at the top.
		top: `${((one.z - view.z) / view.span) * 100}%`
	});

	const round = (value: number) => Math.round(value);

	/** The three the game ships get a name; a modded one keeps its own id. */
	function named(id: string) {
		switch (id) {
			case 'minecraft:overworld':
				return t('map.dimension.overworld');
			case 'minecraft:the_nether':
				return t('map.dimension.the_nether');
			case 'minecraft:the_end':
				return t('map.dimension.the_end');
			default:
				return id.replace('minecraft:', '');
		}
	}

	async function refresh() {
		refreshing = true;
		try {
			installed = await webMap(server.id);
			live = await positions(server.id);
		} catch {
			// The panel says so through the empty state; nothing to add here.
		} finally {
			refreshing = false;
		}
	}

	$effect(() => {
		if (dimensions.length && !dimensions.includes(dimension)) dimension = dimensions[0];
	});
</script>

<svelte:head><title>{t('nav.tabs.map')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto flex w-full max-w-6xl flex-1 flex-col gap-4 px-4 py-6 md:px-8">
	{#if !installed}
		<Skeleton class="h-96 rounded-lg" />
	{:else if installed.found && installed.answering}
		<div class="flex flex-wrap items-center gap-2">
			<Badge variant="secondary">{installed.name}</Badge>
			<span class="text-sm text-muted-foreground">
				{t('map.servedOn', { port: installed.port })}
			</span>
			<Button
				variant="outline"
				size="sm"
				href={mapUrl}
				target="_blank"
				rel="noreferrer"
				class="ml-auto"
			>
				<ExternalLinkIcon />
				{t('map.openTab')}
			</Button>
		</div>
		<iframe
			src={mapUrl}
			title={t('map.title')}
			class="min-h-[32rem] w-full flex-1 rounded-lg border bg-card"
		></iframe>
	{:else}
		{#if installed.found}
			<Alert.Root>
				<Alert.Description>
					{installed.enabled
						? t('map.installedQuiet', { name: installed.name, port: installed.port })
						: t('map.installedOff', { name: installed.name, config: installed.config })}
				</Alert.Description>
			</Alert.Root>
		{/if}

		<div class="flex flex-wrap items-center gap-2">
			<h2 class="text-base font-semibold tracking-tight">{t('map.radar')}</h2>
			{#each dimensions as one (one)}
				<button
					class={[
						'rounded-full border px-2.5 py-0.5 text-xs transition-colors',
						one === dimension ? 'border-foreground/40 bg-accent' : 'text-muted-foreground'
					]}
					aria-pressed={one === dimension}
					onclick={() => (dimension = one)}
				>
					{named(one)}
				</button>
			{/each}
			<Button variant="ghost" size="sm" class="ml-auto" disabled={refreshing} onclick={refresh}>
				<RefreshIcon />
				{t('common.actions.refresh')}
			</Button>
		</div>

		{#if live && !live.supported}
			<Alert.Root>
				<Alert.Description>{live.reason}</Alert.Description>
			</Alert.Root>
		{/if}

		<div class="grid justify-center gap-6 lg:grid-cols-[auto_16rem] lg:items-start">
			<div
				class="radar relative aspect-square h-[calc(100svh-22rem)] max-h-160 min-h-72 w-auto self-center overflow-hidden rounded-lg border bg-card"
				aria-label={t('map.radar')}
			>
				<!-- 16 squares across, so each one is a chunk when the view is at its tightest. -->
				<div class="absolute inset-0 grid grid-cols-8 grid-rows-8 opacity-[0.12]">
					{#each { length: 64 } as _, cell (cell)}
						<div class="border-r border-b border-foreground"></div>
					{/each}
				</div>
				<div class="absolute inset-x-0 top-1/2 h-px bg-foreground/20"></div>
				<div class="absolute inset-y-0 left-1/2 w-px bg-foreground/20"></div>

				<span class="absolute top-1 left-1/2 -translate-x-1/2 text-[10px] text-muted-foreground">
					{t('map.north')}
				</span>
				<span class="absolute bottom-1 left-1 font-mono text-[10px] text-muted-foreground">
					{round(view.x)}, {round(view.z + view.span)}
				</span>
				<span class="absolute top-1 right-1 font-mono text-[10px] text-muted-foreground">
					{round(view.x + view.span)}, {round(view.z)}
				</span>

				{#each shown as one (one.name)}
					{@const place = at(one)}
					<Tooltip.Root>
						<Tooltip.Trigger>
							{#snippet child({ props })}
								<div
									{...props}
									class="absolute -translate-x-1/2 -translate-y-1/2 transition-[left,top] duration-700 ease-linear"
									style="left: {place.left}; top: {place.top}"
								>
									<img
										src={headUrl({ name: one.name })}
										alt=""
										class="size-6 rounded ring-2 ring-background"
									/>
									<span
										class="absolute top-full left-1/2 mt-0.5 -translate-x-1/2 rounded bg-background/80 px-1 text-[10px] whitespace-nowrap"
									>
										{one.name}
									</span>
								</div>
							{/snippet}
						</Tooltip.Trigger>
						<Tooltip.Content>
							{one.name} · {round(one.x)}, {round(one.y)}, {round(one.z)}
						</Tooltip.Content>
					</Tooltip.Root>
				{/each}

				{#if shown.length === 0}
					<p
						class="absolute inset-0 flex items-center justify-center px-6 text-center text-sm text-muted-foreground"
					>
						{running ? t('map.nobody') : t('map.stopped')}
					</p>
				{/if}
			</div>

			{#if shown.length}
				<ul class="divide-y rounded-lg border bg-card text-sm">
					{#each shown as one (one.name)}
						<li class="flex items-center gap-2.5 px-3 py-2">
							<img src={headUrl({ name: one.name })} alt="" class="size-6 rounded" />
							<span class="min-w-0 flex-1 truncate font-medium">{one.name}</span>
							<span class="font-mono text-xs whitespace-nowrap text-muted-foreground">
								{round(one.x)}
								{round(one.y)}
								{round(one.z)}
							</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>

		<p class="self-center text-center text-xs text-balance text-muted-foreground">
			{t('map.radarHint')}
		</p>
	{/if}
</div>
