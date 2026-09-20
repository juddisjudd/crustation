<script lang="ts">
	import BanIcon from '@lucide/svelte/icons/ban';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SendIcon from '@lucide/svelte/icons/send';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import PlayerActions from '$lib/components/player-actions.svelte';
	import { errorMessage } from '$lib/api/servers';
	import {
		actOnPlayer,
		addToList,
		headUrl,
		playerOverview,
		readList,
		removeFromList,
		type PlayerList,
		type PlayerOverview
	} from '$lib/api/players';
	import { dateTime } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);
	const canCommand = $derived(server.permissions.includes('COMMANDS'));

	let overview = $state.raw<PlayerOverview | null>(null);
	let loadFailed = $state(false);
	let chosen = $state('');
	let current = $state.raw<PlayerList | null>(null);
	let busy = $state(false);

	let value = $state('');
	let reason = $state('');
	let broadcast = $state('');

	const label = (slug: string) => t(`players.lists.${slug}` as MessageKey);
	const promptFor = $derived(
		current?.key === 'ip'
			? t('players.add.ip')
			: current?.key === 'xuid'
				? t('players.add.xuid')
				: t('players.add.name')
	);

	/** One row per person, whether they are on now or were on last week. */
	const people = $derived.by(() => {
		if (!overview) return [];
		const rows = new Map<
			string,
			{ name: string; uuid: string | null; last?: string; on: boolean }
		>();
		for (const seen of overview.known) {
			rows.set(seen.name.toLowerCase(), {
				name: seen.name,
				uuid: seen.uuid,
				last: seen.last_seen,
				on: seen.online
			});
		}
		// Somebody on a list but never seen still belongs here: they are exactly
		// who you want to un-ban or de-op without hunting through tabs.
		for (const name of overview.listed) {
			const key = name.toLowerCase();
			if (!rows.has(key)) rows.set(key, { name, uuid: null, on: false });
		}
		for (const here of overview.online ?? []) {
			const key = here.name.toLowerCase();
			const already = rows.get(key);
			rows.set(key, {
				name: here.name,
				uuid: here.uuid ?? already?.uuid ?? null,
				last: already?.last,
				on: true
			});
		}
		return [...rows.values()].sort((a, b) => {
			if (a.on !== b.on) return a.on ? -1 : 1;
			if (a.last !== b.last) return (b.last ?? '').localeCompare(a.last ?? '');
			return a.name.localeCompare(b.name);
		});
	});

	const onNow = $derived(people.filter((one) => one.on).map((one) => one.name));
	const isOperator = (name: string) => !!overview?.operators.includes(name.toLowerCase());
	const isBanned = (name: string) => !!overview?.banned.includes(name.toLowerCase());

	async function load() {
		loadFailed = false;
		try {
			overview = await playerOverview(server.id);
			if (!chosen || !overview.lists.includes(chosen)) chosen = overview.lists[0] ?? '';
		} catch {
			loadFailed = true;
		}
	}

	$effect(() => {
		void server.id;
		load();
	});

	$effect(() => {
		const list = chosen;
		const id = server.id;
		if (!list) return;
		readList(id, list)
			.then((loaded) => {
				if (chosen === list && server.id === id) current = loaded;
			})
			.catch(() => {
				if (chosen === list) current = null;
			});
	});

	async function refresh() {
		await load();
		if (chosen) current = await readList(server.id, chosen).catch(() => current);
	}

	async function add() {
		if (!value.trim() || !chosen) return;
		busy = true;
		try {
			await addToList(server.id, chosen, value.trim(), reason.trim() ? { reason } : {});
			toast.success(t('players.add.added', { name: value.trim() }));
			value = '';
			reason = '';
			await refresh();
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function drop(entry: Record<string, unknown>) {
		const name = identify(entry);
		busy = true;
		try {
			await removeFromList(server.id, chosen, name);
			toast.success(t('players.removed', { name }));
			await refresh();
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function say() {
		const message = broadcast.trim();
		if (!message) return;
		busy = true;
		try {
			await actOnPlayer(server.id, { action: 'say', message });
			broadcast = '';
			toast.success(t('players.act.sent'));
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	/** The one field that names a row, whichever of the three it is. */
	function identify(entry: Record<string, unknown>) {
		const key = current?.key ?? 'name';
		return String(entry[key] ?? entry.name ?? entry.uuid ?? entry.xuid ?? '');
	}

	// Every field a list carries, in the order that reads best. Anything the
	// game adds later lands at the end rather than going missing.
	const ORDER = [
		'name',
		'ip',
		'xuid',
		'level',
		'permission',
		'reason',
		'created',
		'expires',
		'source',
		'bypassesPlayerLimit',
		'ignoresPlayerLimit'
	];

	const columns = $derived.by(() => {
		if (!current) return [];
		const keys = new Set<string>();
		for (const entry of current.entries) for (const key of Object.keys(entry)) keys.add(key);
		keys.delete('uuid');
		keys.delete(current.key);
		const place = (key: string) => {
			const found = ORDER.indexOf(key);
			return found === -1 ? ORDER.length : found;
		};
		return [...keys].sort((a, b) => place(a) - place(b));
	});

	function heading(key: string) {
		const spaced = key.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/[-_]/g, ' ');
		return spaced.charAt(0).toUpperCase() + spaced.slice(1);
	}

	function cell(entry: Record<string, unknown>, key: string) {
		const held = entry[key];
		if (typeof held === 'boolean') return held ? t('common.state.yes') : t('common.state.no');
		if (held === null || held === undefined) return '';
		if (key === 'created') {
			const when = new Date(String(held));
			if (!Number.isNaN(when.getTime())) return dateTime(when.toISOString());
		}
		return String(held);
	}
</script>

<svelte:head><title>{t('players.title')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-6xl px-4 py-6 md:px-8">
	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('players.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !overview}
		<Skeleton class="h-64 rounded-lg" />
	{:else}
		{@const state = overview}
		<div class="grid gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] xl:items-start">
			<section class="rounded-lg border bg-card">
				<div class="flex flex-wrap items-baseline justify-between gap-2 border-b px-5 py-4">
					<h2 class="text-base font-semibold tracking-tight">{t('players.people')}</h2>
					{#if state.count !== null}
						<span class="text-sm text-muted-foreground tabular-nums">
							{t('players.online.count', { count: state.count, max: state.max ?? 0 })}
						</span>
					{/if}
				</div>

				{#if people.length === 0}
					<p class="px-5 py-8 text-center text-sm text-muted-foreground">
						{t('players.online.none')}
					</p>
				{:else}
					<Table.Root>
						<Table.Body>
							{#each people as person (person.name)}
								<Table.Row>
									<Table.Cell class="py-2">
										<div class="flex items-center gap-2.5">
											<img src={headUrl(person)} alt="" class="size-7 rounded" />
											<span class="font-medium">{person.name}</span>
											{#if isOperator(person.name)}
												<Tooltip.Root>
													<Tooltip.Trigger>
														{#snippet child({ props })}
															<Badge {...props} variant="secondary" class="gap-1">
																<ShieldIcon class="size-3" />
																{t('players.badge.operator')}
															</Badge>
														{/snippet}
													</Tooltip.Trigger>
													<Tooltip.Content>{t('players.badge.operatorHint')}</Tooltip.Content>
												</Tooltip.Root>
											{/if}
											{#if isBanned(person.name)}
												<Badge variant="destructive" class="gap-1">
													<BanIcon class="size-3" />
													{t('players.badge.banned')}
												</Badge>
											{/if}
										</div>
									</Table.Cell>
									<Table.Cell
										class="py-2 text-right text-xs whitespace-nowrap text-muted-foreground"
									>
										{#if person.on}
											<span class="text-success">{t('players.known.here')}</span>
										{:else if person.last}
											{dateTime(person.last)}
										{/if}
									</Table.Cell>
									<Table.Cell class="w-10 py-2">
										<PlayerActions
											serverId={server.id}
											edition={state.edition}
											player={person.name}
											operator={isOperator(person.name)}
											banned={isBanned(person.name)}
											running={state.running}
											canManage
											{canCommand}
											others={onNow.filter((one) => one !== person.name)}
											onchange={refresh}
										/>
									</Table.Cell>
								</Table.Row>
							{/each}
						</Table.Body>
					</Table.Root>
				{/if}

				{#if state.sampled && onNow.length}
					<p class="border-t px-5 py-3 text-xs text-muted-foreground">
						{t('players.online.sampled')}
					</p>
				{/if}

				{#if canCommand}
					<form
						class="flex gap-2 border-t px-5 py-3"
						onsubmit={(event) => {
							event.preventDefault();
							say();
						}}
					>
						<Input
							bind:value={broadcast}
							placeholder={t('players.act.sayPlaceholder')}
							aria-label={t('players.act.say')}
							disabled={!state.running}
						/>
						<Button type="submit" disabled={busy || !state.running || !broadcast.trim()}>
							<SendIcon />
							{t('players.act.say')}
						</Button>
					</form>
				{/if}
			</section>

			{#if state.lists.length}
				<section class="rounded-lg border bg-card">
					<div class="border-b px-5 py-4">
						<h2 class="text-base font-semibold tracking-tight">{t('players.listsTitle')}</h2>
					</div>

					<div class="px-5 py-4">
						<Tabs.Root bind:value={chosen}>
							<Tabs.List class="flex-wrap">
								{#each state.lists as slug (slug)}
									<Tabs.Trigger value={slug}>{label(slug)}</Tabs.Trigger>
								{/each}
							</Tabs.List>
						</Tabs.Root>

						<div class="mt-4 flex flex-wrap items-end gap-2">
							<Input
								bind:value
								placeholder={promptFor}
								aria-label={promptFor}
								class="w-40"
								onkeydown={(event) => event.key === 'Enter' && add()}
							/>
							{#if chosen.startsWith('banned')}
								<Input
									bind:value={reason}
									placeholder={t('players.add.reason')}
									aria-label={t('players.add.reason')}
									class="w-40"
								/>
							{/if}
							<Button onclick={add} disabled={busy || !value.trim()}>
								<PlusIcon />
								{t('players.add.button')}
							</Button>
						</div>

						{#if current?.entries.length}
							<div class="mt-4 overflow-x-auto rounded-lg border">
								<Table.Root>
									<Table.Header>
										<Table.Row class="bg-muted/50 hover:bg-muted/50">
											<Table.Head>{heading(current.key)}</Table.Head>
											{#each columns as key (key)}
												<Table.Head class="whitespace-nowrap">{heading(key)}</Table.Head>
											{/each}
											<Table.Head class="w-10"></Table.Head>
										</Table.Row>
									</Table.Header>
									<Table.Body>
										{#each current.entries as entry, index (index)}
											<Table.Row>
												<Table.Cell>
													<div class="flex items-center gap-2">
														{#if current.key !== 'ip'}
															<img
																src={headUrl({
																	uuid: entry.uuid as string | undefined,
																	name: identify(entry)
																})}
																alt=""
																class="size-6 rounded"
															/>
														{/if}
														<span class="font-medium">{identify(entry)}</span>
													</div>
												</Table.Cell>
												{#each columns as key (key)}
													<Table.Cell
														class="max-w-48 truncate text-xs text-muted-foreground"
														title={cell(entry, key)}
													>
														{cell(entry, key)}
													</Table.Cell>
												{/each}
												<Table.Cell class="w-10">
													<Button
														variant="ghost"
														size="icon-sm"
														aria-label={t('players.remove', { name: identify(entry) })}
														disabled={busy}
														onclick={() => drop(entry)}
													>
														<XIcon />
													</Button>
												</Table.Cell>
											</Table.Row>
										{/each}
									</Table.Body>
								</Table.Root>
							</div>
						{:else}
							<p class="mt-4 text-sm text-muted-foreground">{t('players.empty')}</p>
						{/if}

						<p class="mt-3 text-xs text-muted-foreground">
							{t('players.fileHint', { file: current?.file ?? '' })}
						</p>
					</div>
				</section>
			{/if}
		</div>
	{/if}
</div>
