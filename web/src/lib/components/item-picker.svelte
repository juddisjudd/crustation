<script lang="ts">
	import * as Command from '$lib/components/ui/command/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { giveableItems, type GiveableItem } from '$lib/api/players';
	import { errorMessage } from '$lib/api/servers';
	import { t } from '$lib/i18n/index.svelte';

	interface Props {
		serverId: string;
		/** The id that will be given. Any id is allowed; the server is the judge. */
		value: string;
	}

	let { serverId, value = $bindable() }: Props = $props();

	/** Long enough to scroll through, short enough that typing stays instant. */
	const SHOWN = 200;

	let all = $state<GiveableItem[]>([]);
	let source = $state<'server' | 'catalogue' | null>(null);
	let kind = $state('');
	let loading = $state(true);
	let failed = $state<string | null>(null);
	let search = $state('');

	$effect(() => {
		let current = true;
		loading = true;
		failed = null;
		giveableItems(serverId)
			.then((result) => {
				if (!current) return;
				all = result.items ?? [];
				source = result.source;
				kind = result.kind;
			})
			.catch((error) => current && (failed = errorMessage(error)))
			.finally(() => current && (loading = false));
		return () => (current = false);
	});

	/** Whatever was typed is worth offering even when it matches nothing: a pack
	 * can add an item the panel has never heard of. */
	const typed = $derived(search.trim());
	const unknown = $derived(
		typed.length > 0 && !all.some((one) => one.id === typed || one.id === `minecraft:${typed}`)
	);

	const matches = $derived.by(() => {
		const needle = typed.toLowerCase();
		if (!needle) return all;
		const starts: GiveableItem[] = [];
		const holds: GiveableItem[] = [];
		for (const one of all) {
			const id = one.id.toLowerCase();
			const name = one.name.toLowerCase();
			if (id.startsWith(needle) || name.startsWith(needle)) starts.push(one);
			else if (id.includes(needle) || name.includes(needle)) holds.push(one);
		}
		return [...starts, ...holds];
	});

	const shown = $derived(matches.slice(0, SHOWN));
</script>

<div class="grid gap-2">
	<Command.Root shouldFilter={false} class="rounded-lg border bg-transparent p-0">
		<Command.Input placeholder={t('players.act.itemSearch')} bind:value={search} />
		<Command.List class="max-h-64">
			{#if loading}
				<div class="grid gap-1 p-2">
					{#each { length: 6 } as _, row (row)}
						<Skeleton class="h-8 w-full" />
					{/each}
				</div>
			{:else if failed}
				<p class="p-3 text-sm text-destructive">{failed}</p>
			{:else}
				{#if unknown}
					<Command.Item value={typed} onSelect={() => (value = typed)}>
						<span class="font-mono text-sm">{typed}</span>
						<span class="ml-auto text-xs text-muted-foreground">{t('players.act.itemAsTyped')}</span
						>
					</Command.Item>
				{/if}
				{#if !shown.length && !unknown}
					<Command.Empty>{t('players.act.itemNone')}</Command.Empty>
				{/if}
				{#each shown as one (one.id)}
					<Command.Item value={one.id} onSelect={() => (value = one.id)}>
						<span class="truncate">{one.name}</span>
						<span
							class="ml-auto truncate font-mono text-xs {value === one.id
								? 'text-foreground'
								: 'text-muted-foreground'}"
						>
							{one.id}
						</span>
					</Command.Item>
				{/each}
			{/if}
		</Command.List>
	</Command.Root>

	<p class="text-xs text-muted-foreground">
		{#if loading}
			{t('players.act.itemLoading')}
		{:else if failed}
			{t('players.act.itemTypeInstead')}
		{:else if matches.length > shown.length}
			{t('players.act.itemSome', { shown: shown.length, total: matches.length })}
		{:else if source === 'server'}
			{t('players.act.itemFromServer', { total: all.length })}
		{:else if kind === 'minecraft_bedrock'}
			{t('players.act.itemFromCatalogueBedrock', { total: all.length })}
		{:else}
			{t('players.act.itemFromCatalogue', { total: all.length })}
		{/if}
	</p>
</div>
