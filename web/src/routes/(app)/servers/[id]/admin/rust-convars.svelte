<script lang="ts">
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import SearchIcon from '@lucide/svelte/icons/search';
	import Undo2Icon from '@lucide/svelte/icons/undo-2';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { untrack } from 'svelte';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { rustConvars, setRustConvar, type RustConvar, type RustConvars } from '$lib/api/rust';
	import { errorMessage } from '$lib/api/servers';
	import type { Server } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { t, plural } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	const PAGE = 60;

	let listing = $state.raw<RustConvars | null>(null);
	let failed = $state<string | null>(null);
	let loading = $state(false);
	let query = $state('');
	let filter = $state<'all' | 'startup' | 'changed'>('all');
	let group = $state('');
	// Only what was typed and not yet sent, keyed by convar name.
	let drafts = $state<Record<string, string>>({});
	let busy = $state<string | null>(null);

	const running = $derived(servers.isLive(server.id));
	const ready = $derived(servers.statusOf(server.id) === 'running');
	const live = $derived(!!listing?.live && running);
	const canKeep = $derived(listing?.can_keep ?? false);

	async function load() {
		loading = true;
		failed = null;
		try {
			listing = await rustConvars(server.id);
		} catch (error) {
			failed = errorMessage(error);
		} finally {
			loading = false;
		}
	}

	// Starting or stopping changes where every value comes from.
	$effect(() => {
		void ready;
		void server.id;
		untrack(load);
	});

	/** Rust answers True and False; the start command and the panel use true and false. */
	function normal(convar: RustConvar, value: string | null) {
		if (value === null) return null;
		if (convar.type !== 'bool') return value;
		return String(value.toLowerCase() === 'true' || value === '1');
	}

	const current = (convar: RustConvar) =>
		normal(convar, convar.value ?? convar.startup ?? convar.default) ?? '';
	const shown = (convar: RustConvar) => drafts[convar.name] ?? current(convar);
	const dirty = (convar: RustConvar) =>
		convar.name in drafts && drafts[convar.name] !== current(convar);

	function differs(convar: RustConvar) {
		const value = normal(convar, convar.value ?? convar.startup);
		const fallback = normal(convar, convar.default);
		if (value === null || fallback === null) return false;
		if (convar.type === 'int' || convar.type === 'float') return Number(value) !== Number(fallback);
		return value !== fallback;
	}

	const groups = $derived.by(() => {
		const counts: Record<string, number> = {};
		for (const convar of listing?.convars ?? []) {
			const prefix = convar.name.split('.')[0];
			counts[prefix] = (counts[prefix] ?? 0) + 1;
		}
		return Object.entries(counts).sort(([a], [b]) => a.localeCompare(b));
	});

	const needle = $derived(query.trim().toLowerCase());
	// Back to the first page whenever what is shown changes.
	let limit = $derived.by(() => {
		void needle;
		void filter;
		void group;
		return PAGE;
	});
	const matches = $derived(
		(listing?.convars ?? []).filter(
			(convar) =>
				(!group || convar.name.startsWith(`${group}.`)) &&
				(filter !== 'startup' || convar.startup !== null) &&
				(filter !== 'changed' || differs(convar)) &&
				(!needle || convar.name.includes(needle) || convar.help.toLowerCase().includes(needle))
		)
	);
	const visible = $derived(matches.slice(0, limit));

	function shownValue(convar: RustConvar, value: string | null) {
		if (convar.type === 'bool') {
			return normal(convar, value) === 'true'
				? t('rustAdmin.convars.on')
				: t('rustAdmin.convars.off');
		}
		return value === '' ? '""' : (value ?? '');
	}

	function patch(name: string, change: Partial<RustConvar>) {
		if (!listing) return;
		listing = {
			...listing,
			convars: listing.convars.map((one) => (one.name === name ? { ...one, ...change } : one))
		};
	}

	async function apply(convar: RustConvar, mode: 'now' | 'keep' | 'drop') {
		const value = mode === 'drop' ? null : shown(convar);
		const now = mode === 'now' || (mode === 'keep' && live);
		busy = convar.name;
		try {
			const result = await setRustConvar(server.id, convar.name, {
				value,
				now,
				startup: mode !== 'now'
			});
			patch(convar.name, {
				value: result.live ?? convar.value,
				startup: result.startup
			});
			delete drafts[convar.name];
			const name = convar.name;
			const said = shownValue(convar, result.live ?? value);
			toast.success(
				mode === 'drop'
					? t('rustAdmin.convars.dropped', { name })
					: mode === 'now'
						? t('rustAdmin.convars.setDone', { name, value: said })
						: now
							? t('rustAdmin.convars.keptNow', { name, value: said })
							: t('rustAdmin.convars.keptLater', { name })
			);
		} catch (error) {
			toast.error(t('rustAdmin.convars.failed', { name: convar.name }), {
				description: errorMessage(error)
			});
		} finally {
			busy = null;
		}
	}
</script>

<section class="rounded-lg border bg-card">
	<div class="flex flex-wrap items-center gap-2 border-b px-5 py-3">
		<div class="relative w-full sm:w-72">
			<SearchIcon
				class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
			/>
			<Input
				type="search"
				bind:value={query}
				placeholder={t('rustAdmin.search')}
				aria-label={t('rustAdmin.search')}
				onkeydown={(event) => event.key === 'Escape' && (query = '')}
				class="pl-8"
			/>
		</div>
		<Tabs.Root bind:value={filter}>
			<Tabs.List>
				{#each ['all', 'startup', 'changed'] as const as one (one)}
					<Tabs.Trigger value={one}>{t(`rustAdmin.convars.filters.${one}`)}</Tabs.Trigger>
				{/each}
			</Tabs.List>
		</Tabs.Root>
		<Select.Root type="single" bind:value={group}>
			<Select.Trigger class="w-44">
				{group || t('rustAdmin.convars.allGroups')}
			</Select.Trigger>
			<Select.Content class="max-h-80">
				<Select.Item value="" label={t('rustAdmin.convars.allGroups')} />
				{#each groups as [prefix, count] (prefix)}
					<Select.Item value={prefix} label={prefix}>
						{prefix}
						<span class="ml-auto text-xs text-muted-foreground tabular-nums">{count}</span>
					</Select.Item>
				{/each}
			</Select.Content>
		</Select.Root>
		<div class="ml-auto flex items-center gap-2">
			{#if listing}
				<span class="text-xs text-muted-foreground tabular-nums">
					{plural('rustAdmin.convars.count', matches.length)}
				</span>
			{/if}
			<Button
				variant="ghost"
				size="icon-sm"
				disabled={loading}
				onclick={load}
				aria-label={t('rustAdmin.refresh')}
				title={t('rustAdmin.refresh')}
			>
				<RefreshCwIcon class={loading ? 'animate-spin' : undefined} />
			</Button>
		</div>
	</div>

	{#if failed}
		<Alert.Root variant="destructive" class="m-5 w-auto">
			<Alert.Title>{t('rustAdmin.failed')}</Alert.Title>
			<Alert.Description>{failed}</Alert.Description>
		</Alert.Root>
	{:else if !listing}
		<div class="space-y-3 p-5">
			{#each [0, 1, 2, 3, 4] as row (row)}
				<Skeleton class="h-12 w-full" />
			{/each}
		</div>
	{:else}
		<p class="border-b bg-muted/40 px-5 py-2 text-xs text-muted-foreground">
			{live
				? t('rustAdmin.convars.live')
				: running
					? t('rustAdmin.convars.starting')
					: t('rustAdmin.convars.stopped')}
		</p>

		{#if !matches.length}
			<p class="px-5 py-10 text-center text-sm text-muted-foreground">{t('rustAdmin.noMatch')}</p>
		{:else}
			<ul class="divide-y">
				{#each visible as convar (convar.name)}
					{@const editing = dirty(convar)}
					{@const working = busy === convar.name}
					<li
						class="grid gap-x-6 gap-y-2 px-5 py-3 lg:grid-cols-[minmax(0,1fr)_minmax(0,24rem)] lg:items-center"
					>
						<div class="min-w-0">
							<div class="flex flex-wrap items-center gap-x-2 gap-y-1">
								<span class="font-mono text-sm font-medium break-all">{convar.name}</span>
								{#if convar.startup !== null}
									<Badge variant="secondary">{t('rustAdmin.convars.onStartup')}</Badge>
								{/if}
								{#if convar.managed}
									<Badge variant="outline">{t('rustAdmin.convars.managed')}</Badge>
								{/if}
							</div>
							{#if convar.help}
								<p
									class="mt-0.5 text-sm text-muted-foreground"
									title={convar.generated ? t('rustAdmin.generated') : undefined}
								>
									{convar.help}
								</p>
							{/if}
							{#if convar.default !== null}
								<p class="mt-0.5 text-xs text-muted-foreground">
									{t('rustAdmin.convars.default', { value: shownValue(convar, convar.default) })}
								</p>
							{/if}
						</div>

						<div class="flex items-center gap-2">
							{#if convar.type === 'bool'}
								<Switch
									checked={shown(convar) === 'true'}
									disabled={convar.managed || working}
									onCheckedChange={(on) => (drafts[convar.name] = String(on))}
									aria-label={convar.name}
								/>
								<span class="text-sm">{shownValue(convar, shown(convar))}</span>
							{:else}
								<Input
									type={convar.type === 'int' || convar.type === 'float' ? 'number' : 'text'}
									step="any"
									value={shown(convar)}
									disabled={convar.managed || working}
									oninput={(event) => (drafts[convar.name] = event.currentTarget.value)}
									onkeydown={(event) => {
										if (event.key === 'Escape') delete drafts[convar.name];
									}}
									aria-label={convar.name}
									class="min-w-0 flex-1 font-mono text-xs"
								/>
							{/if}
							<div class="ml-auto flex shrink-0 items-center gap-1">
								{#if editing}
									<Button
										variant="ghost"
										size="icon-sm"
										disabled={working}
										onclick={() => delete drafts[convar.name]}
										aria-label={t('rustAdmin.convars.undo')}
										title={t('rustAdmin.convars.undo')}
									>
										<Undo2Icon />
									</Button>
									<Button
										variant="outline"
										size="sm"
										disabled={!live || working}
										onclick={() => apply(convar, 'now')}
										title={t('rustAdmin.convars.setNowHint')}
									>
										{t('rustAdmin.convars.setNow')}
									</Button>
									{#if canKeep}
										<Button
											size="sm"
											disabled={working}
											onclick={() => apply(convar, 'keep')}
											title={t('rustAdmin.convars.keepHint')}
										>
											{t('rustAdmin.convars.keep')}
										</Button>
									{/if}
								{:else if convar.startup !== null && canKeep && !convar.managed}
									<Button
										variant="ghost"
										size="icon-sm"
										disabled={working}
										onclick={() => apply(convar, 'drop')}
										aria-label={t('rustAdmin.convars.drop', { name: convar.name })}
										title={t('rustAdmin.convars.drop', { name: convar.name })}
									>
										<XIcon />
									</Button>
								{/if}
							</div>
						</div>
					</li>
				{/each}
			</ul>
			{#if matches.length > visible.length}
				<div class="border-t p-3 text-center">
					<Button variant="ghost" size="sm" onclick={() => (limit += 100)}>
						{t('rustAdmin.showMore', { count: Math.min(100, matches.length - visible.length) })}
					</Button>
				</div>
			{/if}
		{/if}
	{/if}
</section>
