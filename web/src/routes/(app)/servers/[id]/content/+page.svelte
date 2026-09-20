<script lang="ts">
	import BoxIcon from '@lucide/svelte/icons/box';
	import GlobeIcon from '@lucide/svelte/icons/globe';
	import PackagePlusIcon from '@lucide/svelte/icons/package-plus';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as RadioGroup from '$lib/components/ui/radio-group/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { errorMessage } from '$lib/api/servers';
	import {
		ADDON_TYPES,
		WORLD_TYPES,
		listPacks,
		listWorlds,
		playWorld,
		uploadPack,
		type Pack,
		type PackSort,
		type World
	} from '$lib/api/packs';
	import { t } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);

	let packs = $state.raw<Pack[] | null>(null);
	let worlds = $state.raw<World[] | null>(null);
	let levelName = $state('');
	let loadFailed = $state(false);
	let busy = $state(false);

	let addonInput = $state.raw<HTMLInputElement | null>(null);
	let worldInput = $state.raw<HTMLInputElement | null>(null);

	/** An add-on waiting on the answer to "which world?". */
	let pending = $state.raw<File | null>(null);
	let target = $state('');

	async function refresh(id: string) {
		try {
			const [found, keeps] = await Promise.all([listPacks(id), listWorlds(id)]);
			if (server.id !== id) return;
			packs = found.packs;
			worlds = keeps.worlds;
			levelName = keeps.level_name;
			loadFailed = false;
		} catch {
			if (server.id !== id) return;
			loadFailed = true;
		}
	}

	$effect(() => {
		const id = server.id;
		packs = null;
		worlds = null;
		refresh(id);
	});

	const sortLabel = (sort: PackSort) =>
		({
			behaviour: t('content.sortBehaviour'),
			resource: t('content.sortResource'),
			datapack: t('content.sortDatapack'),
			skin: t('content.sortSkin'),
			world_template: t('content.sortWorldTemplate'),
			world: t('content.sortWorld')
		})[sort];

	function chosen(event: Event & { currentTarget: HTMLInputElement }) {
		const file = event.currentTarget.files?.[0];
		event.currentTarget.value = '';
		if (!file) return;

		// Only a Bedrock pack is keyed to one world, and only a server with
		// several leaves anything to choose.
		if ((worlds?.length ?? 0) > 1) {
			target = levelName || worlds![0].folder;
			pending = file;
			return;
		}
		install(file);
	}

	async function install(file: File, world?: string) {
		busy = true;
		try {
			const done = await uploadPack(server.id, file, { world, use_world: false });
			const names = done.installed.map((one) => one.name).join(', ');

			// A pack whose files landed but whose world would not take it is worth
			// saying out loud: the install looks like it worked and the game will
			// not load it.
			const quiet = done.installed.some((one) => one.sort !== 'world' && !one.activated);
			if (quiet) {
				toast.warning(t('content.installed', { names }), {
					description: t('content.notSwitchedOn', { world: done.world })
				});
			} else {
				toast.success(t('content.installed', { names }), {
					description:
						(worlds?.length ?? 0) > 1
							? t('content.installedFor', { world: done.world })
							: t('content.installedWhere')
				});
			}
			await refresh(server.id);
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function importWorld(event: Event & { currentTarget: HTMLInputElement }) {
		const file = event.currentTarget.files?.[0];
		event.currentTarget.value = '';
		if (!file) return;

		busy = true;
		try {
			const done = await uploadPack(server.id, file, { use_world: false });
			const names = done.installed.map((one) => one.name).join(', ');
			toast.success(t('content.worldImported', { names }), {
				description: t('content.worldKept')
			});
			await refresh(server.id);
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function play(world: World) {
		busy = true;
		try {
			await playWorld(server.id, world.folder);
			levelName = world.folder;
			toast.success(t('content.nowPlaying', { world: world.name }));
			await refresh(server.id);
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	function confirmWorld() {
		const file = pending;
		pending = null;
		if (file) install(file, target);
	}
</script>

<svelte:head><title>{t('content.title')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-6xl space-y-6 px-4 py-6 md:px-8">
	<div class="flex flex-wrap items-start justify-between gap-4">
		<p class="max-w-xl text-sm text-muted-foreground">{t('content.hint')}</p>

		<div class="flex items-center gap-2">
			{#if busy}<Spinner class="size-4" />{/if}
			<Button size="sm" disabled={busy} onclick={() => addonInput?.click()}>
				<PackagePlusIcon />
				{t('content.installAddon')}
			</Button>
			<Button variant="outline" size="sm" disabled={busy} onclick={() => worldInput?.click()}>
				<GlobeIcon />
				{t('content.importWorld')}
			</Button>
			<input
				bind:this={addonInput}
				type="file"
				class="hidden"
				accept={ADDON_TYPES}
				onchange={chosen}
			/>
			<input
				bind:this={worldInput}
				type="file"
				class="hidden"
				accept={WORLD_TYPES}
				onchange={importWorld}
			/>
		</div>
	</div>

	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('content.failed')}</Alert.Description>
		</Alert.Root>
	{/if}

	<section class="space-y-3">
		<h2 class="text-sm font-medium">{t('content.packs')}</h2>
		{#if !packs}
			<Skeleton class="h-40 rounded-lg" />
		{:else if packs.length === 0}
			<Empty.Root class="rounded-lg border border-dashed py-12">
				<Empty.Header>
					<Empty.Media variant="icon"><BoxIcon /></Empty.Media>
					<Empty.Title>{t('content.noPacks')}</Empty.Title>
					<Empty.Description>{t('content.noPacksHint')}</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{:else}
			<div class="overflow-hidden rounded-lg border">
				<Table.Root>
					<Table.Header>
						<Table.Row class="bg-muted/50 hover:bg-muted/50">
							<Table.Head>{t('content.packs')}</Table.Head>
							<Table.Head class="hidden w-40 md:table-cell">{t('content.folder')}</Table.Head>
							<Table.Head class="w-28">{t('content.active')}</Table.Head>
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each packs as pack (pack.path)}
							<Table.Row>
								<Table.Cell>
									<div class="font-medium">{pack.name}</div>
									<div class="text-xs text-muted-foreground">
										{sortLabel(pack.sort)}
										{#if pack.version.length}
											· {t('content.version', { version: pack.version.join('.') })}
										{/if}
									</div>
								</Table.Cell>
								<Table.Cell class="hidden truncate font-mono text-xs md:table-cell">
									{pack.path}
								</Table.Cell>
								<Table.Cell>
									{#if pack.activated}
										<Badge class="bg-success text-background">{t('content.active')}</Badge>
									{:else}
										<Badge variant="outline">{t('content.inactive')}</Badge>
									{/if}
								</Table.Cell>
							</Table.Row>
						{/each}
					</Table.Body>
				</Table.Root>
			</div>
		{/if}
	</section>

	<section class="space-y-3">
		<h2 class="text-sm font-medium">{t('content.worlds')}</h2>
		{#if !worlds}
			<Skeleton class="h-32 rounded-lg" />
		{:else if worlds.length === 0}
			<Empty.Root class="rounded-lg border border-dashed py-12">
				<Empty.Header>
					<Empty.Media variant="icon"><GlobeIcon /></Empty.Media>
					<Empty.Title>{t('content.noWorlds')}</Empty.Title>
					<Empty.Description>{t('content.noWorldsHint')}</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{:else}
			<div class="overflow-hidden rounded-lg border">
				<Table.Root>
					<Table.Header>
						<Table.Row class="bg-muted/50 hover:bg-muted/50">
							<Table.Head>{t('content.worlds')}</Table.Head>
							<Table.Head class="hidden w-40 md:table-cell">{t('content.folder')}</Table.Head>
							<Table.Head class="w-40"></Table.Head>
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each worlds as world (world.folder)}
							<Table.Row>
								<Table.Cell class="font-medium">{world.name}</Table.Cell>
								<Table.Cell class="hidden truncate font-mono text-xs md:table-cell">
									{world.path}
								</Table.Cell>
								<Table.Cell class="text-right">
									{#if world.folder === levelName}
										<Badge class="bg-success text-background">{t('content.playing')}</Badge>
									{:else}
										<Button variant="outline" size="sm" disabled={busy} onclick={() => play(world)}>
											{t('content.playThis')}
										</Button>
									{/if}
								</Table.Cell>
							</Table.Row>
						{/each}
					</Table.Body>
				</Table.Root>
			</div>
		{/if}
	</section>
</div>

<Dialog.Root
	open={pending !== null}
	onOpenChange={(open) => {
		if (!open) pending = null;
	}}
>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title>{t('content.chooseWorld')}</Dialog.Title>
			<Dialog.Description>{t('content.chooseWorldBody')}</Dialog.Description>
		</Dialog.Header>

		<RadioGroup.Root bind:value={target} class="gap-3">
			{#each worlds ?? [] as world (world.folder)}
				<div class="flex items-center gap-3">
					<RadioGroup.Item value={world.folder} id={`world-${world.folder}`} />
					<Label for={`world-${world.folder}`} class="font-normal">
						{world.name}
						{#if world.folder === levelName}
							<span class="text-xs text-muted-foreground">· {t('content.playing')}</span>
						{/if}
					</Label>
				</div>
			{/each}
		</RadioGroup.Root>

		<Dialog.Footer>
			<Button variant="outline" onclick={() => (pending = null)}>{t('content.cancel')}</Button>
			<Button onclick={confirmWorld}>{t('content.chooseWorldConfirm')}</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
