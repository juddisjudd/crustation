<script lang="ts">
	import PlusIcon from '@lucide/svelte/icons/plus';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { errorMessage } from '$lib/api/servers';
	import {
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

	let overview = $state.raw<PlayerOverview | null>(null);
	let loadFailed = $state(false);
	let chosen = $state('');
	let current = $state.raw<PlayerList | null>(null);
	let busy = $state(false);

	let value = $state('');
	let reason = $state('');

	const label = (slug: string) => t(`players.lists.${slug}` as MessageKey);
	const promptFor = $derived(
		current?.key === 'ip'
			? t('players.add.ip')
			: current?.key === 'xuid'
				? t('players.add.xuid')
				: t('players.add.name')
	);

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

	async function refreshList() {
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
			await refreshList();
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
			await refreshList();
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

	/** Everything else worth showing beside the name. */
	function details(entry: Record<string, unknown>) {
		const key = current?.key ?? 'name';
		return Object.entries(entry)
			.filter(([field]) => field !== key && field !== 'uuid')
			.map(([field, held]) => `${field}: ${held}`)
			.join(' · ');
	}
</script>

<svelte:head><title>{t('players.title')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-4xl px-4 py-6 md:px-8">
	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('players.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !overview}
		<Skeleton class="h-64 rounded-lg" />
	{:else}
		<section class="rounded-lg border bg-card p-6">
			<div class="flex flex-wrap items-baseline justify-between gap-2">
				<h2 class="text-base font-semibold tracking-tight">{t('players.online.title')}</h2>
				{#if overview.count !== null}
					<span class="text-sm text-muted-foreground tabular-nums">
						{t('players.online.count', { count: overview.count, max: overview.max ?? 0 })}
					</span>
				{/if}
			</div>

			{#if overview.online?.length}
				<ul class="mt-4 flex flex-wrap gap-2">
					{#each overview.online as player (player.name)}
						<li class="flex items-center gap-2 rounded-full border py-1 pr-3 pl-1">
							<img src={headUrl(player)} alt="" class="size-6 rounded-full" />
							<span class="text-sm">{player.name}</span>
						</li>
					{/each}
				</ul>
			{:else}
				<p class="mt-4 text-sm text-muted-foreground">{t('players.online.none')}</p>
			{/if}

			{#if overview.sampled}
				<p class="mt-3 text-xs text-muted-foreground">{t('players.online.sampled')}</p>
			{/if}
		</section>

		{#if overview.lists.length}
			<section class="mt-6 rounded-lg border bg-card p-6">
				<Tabs.Root bind:value={chosen}>
					<Tabs.List>
						{#each overview.lists as slug (slug)}
							<Tabs.Trigger value={slug}>{label(slug)}</Tabs.Trigger>
						{/each}
					</Tabs.List>
				</Tabs.Root>

				<div class="mt-4 flex flex-wrap items-end gap-2">
					<Input
						bind:value
						placeholder={promptFor}
						aria-label={promptFor}
						class="w-48"
						onkeydown={(event) => event.key === 'Enter' && add()}
					/>
					{#if chosen.startsWith('banned')}
						<Input
							bind:value={reason}
							placeholder={t('players.add.reason')}
							aria-label={t('players.add.reason')}
							class="w-48"
						/>
					{/if}
					<Button onclick={add} disabled={busy || !value.trim()}>
						<PlusIcon />
						{t('players.add.button')}
					</Button>
				</div>

				{#if current?.entries.length}
					<div class="mt-4 overflow-hidden rounded-lg border">
						<Table.Root>
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
										<Table.Cell class="hidden text-xs text-muted-foreground md:table-cell">
											{details(entry)}
										</Table.Cell>
										<Table.Cell class="w-12">
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
			</section>
		{/if}

		<section class="mt-6 rounded-lg border bg-card p-6">
			<h2 class="text-base font-semibold tracking-tight">{t('players.known.title')}</h2>
			{#if overview.known.length === 0}
				<p class="mt-4 text-sm text-muted-foreground">{t('players.known.none')}</p>
			{:else}
				<div class="mt-4 overflow-hidden rounded-lg border">
					<Table.Root>
						<Table.Header>
							<Table.Row class="bg-muted/50 hover:bg-muted/50">
								<Table.Head>{t('players.known.name')}</Table.Head>
								<Table.Head class="hidden md:table-cell">{t('players.known.first')}</Table.Head>
								<Table.Head>{t('players.known.last')}</Table.Head>
							</Table.Row>
						</Table.Header>
						<Table.Body>
							{#each overview.known as player (player.name)}
								<Table.Row>
									<Table.Cell>
										<div class="flex items-center gap-2">
											<img src={headUrl(player)} alt="" class="size-6 rounded" />
											<span class="font-medium">{player.name}</span>
											{#if player.online}
												<span class="text-xs text-success">{t('players.known.here')}</span>
											{/if}
										</div>
									</Table.Cell>
									<Table.Cell class="hidden text-sm text-muted-foreground md:table-cell">
										{dateTime(player.first_seen)}
									</Table.Cell>
									<Table.Cell class="text-sm text-muted-foreground">
										{dateTime(player.last_seen)}
									</Table.Cell>
								</Table.Row>
							{/each}
						</Table.Body>
					</Table.Root>
				</div>
			{/if}
		</section>
	{/if}
</div>
