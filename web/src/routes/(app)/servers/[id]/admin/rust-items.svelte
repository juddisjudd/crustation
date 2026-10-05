<script lang="ts">
	import CopyIcon from '@lucide/svelte/icons/copy';
	import GiftIcon from '@lucide/svelte/icons/gift';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import {
		giveableItems,
		itemIconUrl,
		RUST_ITEM_ORDER,
		type GiveableItem,
		type ItemCatalogue
	} from '$lib/api/players';
	import { actOnRustPlayer, rustPlayers, type RustOnline } from '$lib/api/rust';
	import { errorMessage } from '$lib/api/servers';
	import type { Server } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { t, plural, type MessageKey } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	const PAGE = 120;
	const EVERYONE = 'everyone';
	const AMOUNTS = [1, 10, 100, 1000];

	let catalogue = $state.raw<ItemCatalogue | null>(null);
	let failed = $state<string | null>(null);
	let query = $state('');
	let category = $state<string | null>(null);
	let chosen = $state<GiveableItem | null>(null);
	let online = $state.raw<RustOnline[]>([]);
	let target = $state(EVERYONE);
	let amount = $state(1);
	let giving = $state(false);

	const running = $derived(servers.isLive(server.id));

	$effect(() => {
		const id = server.id;
		let current = true;
		failed = null;
		giveableItems(id)
			.then((found) => current && (catalogue = found))
			.catch((error) => current && (failed = errorMessage(error)));
		return () => (current = false);
	});

	const categories = $derived.by(() => {
		const present = new Set((catalogue?.items ?? []).map((one) => one.category));
		return RUST_ITEM_ORDER.filter((one) => present.has(one));
	});

	const needle = $derived(query.trim().toLowerCase());
	// Back to the first page whenever what is shown changes.
	let limit = $derived.by(() => {
		void needle;
		void category;
		return PAGE;
	});
	const matches = $derived(
		(catalogue?.items ?? [])
			.filter(
				(item) =>
					(!category || item.category === category) &&
					(!needle || item.id.includes(needle) || item.name.toLowerCase().includes(needle))
			)
			.sort((a, b) => a.name.localeCompare(b.name))
	);
	const visible = $derived(matches.slice(0, limit));

	/** A picture that will not load leaves its space rather than a broken icon. */
	function hide(event: Event) {
		(event.currentTarget as HTMLImageElement).style.visibility = 'hidden';
	}

	async function choose(item: GiveableItem) {
		chosen = item;
		amount = 1;
		target = EVERYONE;
		online = [];
		if (!running) return;
		try {
			online = (await rustPlayers(server.id)).online;
			target = online[0]?.steam_id ?? EVERYONE;
		} catch {
			// Without PLAYERS the list stays empty and only everyone online is offered.
		}
	}

	async function copy(name: string) {
		try {
			await navigator.clipboard.writeText(name);
			toast.success(t('rustAdmin.items.copied', { name }));
		} catch {
			// clipboard unavailable
		}
	}

	async function give() {
		if (!chosen) return;
		const item = chosen;
		const count = Math.max(1, Math.floor(Number(amount) || 1));
		giving = true;
		try {
			const result = await actOnRustPlayer(
				server.id,
				target === EVERYONE
					? { action: 'give-all', item: item.id, amount: count }
					: { action: 'give', steam_id: target, item: item.id, amount: count }
			);
			const person = online.find((one) => one.steam_id === target)?.name ?? target;
			toast.success(
				target === EVERYONE
					? t('rustAdmin.items.givenAll', { count, item: item.name })
					: t('rustAdmin.items.given', { count, item: item.name, name: person }),
				{ description: result.output?.trim() || undefined }
			);
			chosen = null;
		} catch (error) {
			toast.error(t('rustAdmin.items.failed'), { description: errorMessage(error) });
		} finally {
			giving = false;
		}
	}

	const categoryLabel = (key: string) => t(`players.act.cat.${key}` as MessageKey);
	const targetLabel = $derived(
		target === EVERYONE
			? t('rustAdmin.items.everyone')
			: (online.find((one) => one.steam_id === target)?.name ?? target)
	);
</script>

<section class="rounded-lg border bg-card">
	<div class="space-y-3 border-b px-5 py-3">
		<div class="flex flex-wrap items-center gap-2">
			<div class="relative w-full sm:w-72">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					bind:value={query}
					placeholder={t('players.act.itemSearch')}
					aria-label={t('players.act.itemSearch')}
					onkeydown={(event) => event.key === 'Escape' && (query = '')}
					class="pl-8"
				/>
			</div>
			{#if catalogue}
				<span class="ml-auto text-xs text-muted-foreground tabular-nums">
					{plural('rustAdmin.items.count', matches.length)}
				</span>
			{/if}
		</div>
		{#if categories.length > 1}
			<div class="-mx-1 flex [scrollbar-width:none] gap-1 overflow-x-auto px-1">
				{#each [null, ...categories] as one (one ?? 'all')}
					<button
						type="button"
						aria-pressed={category === one}
						onclick={() => (category = one)}
						class={[
							'shrink-0 rounded-full border px-3 py-1 text-xs transition-colors',
							category === one
								? 'border-primary bg-primary text-primary-foreground'
								: 'text-muted-foreground hover:text-foreground'
						]}
					>
						{one ? categoryLabel(one) : t('rustAdmin.items.all')}
					</button>
				{/each}
			</div>
		{/if}
	</div>

	{#if failed}
		<Alert.Root variant="destructive" class="m-5 w-auto">
			<Alert.Title>{t('rustAdmin.failed')}</Alert.Title>
			<Alert.Description>{failed}</Alert.Description>
		</Alert.Root>
	{:else if !catalogue}
		<div class="grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-2 p-5">
			{#each { length: 12 } as _, tile (tile)}
				<Skeleton class="h-32 rounded-lg" />
			{/each}
		</div>
	{:else}
		{#if !running}
			<p class="border-b bg-muted/40 px-5 py-2 text-xs text-muted-foreground">
				{t('rustAdmin.items.stopped')}
			</p>
		{/if}
		{#if !matches.length}
			<p class="px-5 py-10 text-center text-sm text-muted-foreground">{t('rustAdmin.noMatch')}</p>
		{:else}
			<div class="grid grid-cols-[repeat(auto-fill,minmax(8rem,1fr))] gap-2 p-5">
				{#each visible as item (item.id)}
					{@const picture = itemIconUrl(item, catalogue.kind)}
					<button
						type="button"
						onclick={() => choose(item)}
						class="flex flex-col items-center gap-1.5 rounded-lg border p-3 text-center transition-colors hover:border-foreground/30 hover:bg-accent/40"
					>
						{#if picture}
							<img
								src={picture}
								alt=""
								loading="lazy"
								decoding="async"
								onerror={hide}
								class="size-14 object-contain"
							/>
						{/if}
						<span class="line-clamp-2 text-xs font-medium">{item.name}</span>
						<span class="w-full truncate font-mono text-[11px] text-muted-foreground">
							{item.id}
						</span>
					</button>
				{/each}
			</div>
			{#if matches.length > visible.length}
				<div class="border-t p-3 text-center">
					<Button variant="ghost" size="sm" onclick={() => (limit += PAGE)}>
						{t('rustAdmin.showMore', { count: Math.min(PAGE, matches.length - visible.length) })}
					</Button>
				</div>
			{/if}
		{/if}
	{/if}
</section>

<Dialog.Root open={!!chosen} onOpenChange={(open) => !open && (chosen = null)}>
	<Dialog.Content class="sm:max-w-md">
		{#if chosen}
			{@const picture = itemIconUrl(chosen, catalogue?.kind ?? 'rust')}
			<Dialog.Header>
				<div class="flex items-center gap-3">
					{#if picture}
						<img src={picture} alt="" onerror={hide} class="size-12 object-contain" />
					{/if}
					<div class="min-w-0">
						<Dialog.Title>{chosen.name}</Dialog.Title>
						<Dialog.Description class="flex items-center gap-1 font-mono text-xs">
							{chosen.id}
							<button
								type="button"
								class="rounded p-0.5 text-muted-foreground hover:text-foreground"
								onclick={() => chosen && copy(chosen.id)}
								aria-label={t('rustAdmin.items.copy')}
								title={t('rustAdmin.items.copy')}
							>
								<CopyIcon class="size-3" />
							</button>
						</Dialog.Description>
					</div>
				</div>
			</Dialog.Header>

			<div class="grid gap-4">
				<div class="grid gap-2">
					<Label for="give-to">{t('rustAdmin.items.to')}</Label>
					<Select.Root type="single" bind:value={target} disabled={!running}>
						<Select.Trigger id="give-to" class="w-full">{targetLabel}</Select.Trigger>
						<Select.Content>
							{#each online as person (person.steam_id)}
								<Select.Item value={person.steam_id} label={person.name} />
							{/each}
							<Select.Item value={EVERYONE} label={t('rustAdmin.items.everyone')} />
						</Select.Content>
					</Select.Root>
					{#if running && !online.length}
						<p class="text-xs text-muted-foreground">{t('rustAdmin.items.noPlayers')}</p>
					{/if}
				</div>
				<div class="grid gap-2">
					<Label for="give-amount">{t('players.act.count')}</Label>
					<div class="flex flex-wrap items-center gap-2">
						<Input
							id="give-amount"
							type="number"
							min="1"
							bind:value={amount}
							class="w-28 tabular-nums"
						/>
						{#each AMOUNTS as preset (preset)}
							<Button
								variant={amount === preset ? 'secondary' : 'outline'}
								size="sm"
								onclick={() => (amount = preset)}
							>
								{preset.toLocaleString()}
							</Button>
						{/each}
					</div>
				</div>
				{#if !running}
					<p class="text-sm text-muted-foreground">{t('rustAdmin.items.stopped')}</p>
				{/if}
			</div>

			<Dialog.Footer>
				<Button variant="outline" onclick={() => (chosen = null)}>
					{t('common.actions.cancel')}
				</Button>
				<Button disabled={!running || giving} onclick={give}>
					{#if giving}<Spinner class="size-4" />{:else}<GiftIcon />{/if}
					{t('rustAdmin.items.give')}
				</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
