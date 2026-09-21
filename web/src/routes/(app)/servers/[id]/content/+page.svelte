<script lang="ts">
	import BoxIcon from '@lucide/svelte/icons/box';
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import NotebookPenIcon from '@lucide/svelte/icons/notebook-pen';
	import TrashIcon from '@lucide/svelte/icons/trash-2';
	import TriangleAlertIcon from '@lucide/svelte/icons/triangle-alert';
	import GlobeIcon from '@lucide/svelte/icons/globe';
	import PackagePlusIcon from '@lucide/svelte/icons/package-plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as InputGroup from '$lib/components/ui/input-group/index.js';
	import * as RadioGroup from '$lib/components/ui/radio-group/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import PackNoteDialog from '$lib/components/servers/pack-note-dialog.svelte';
	import { servers } from '$lib/servers.svelte';
	import { errorMessage } from '$lib/api/servers';
	import {
		ADDON_TYPES,
		WORLD_TYPES,
		forgetPack,
		listPacks,
		listWorlds,
		playWorld,
		removePack,
		uploadPack,
		type MissingPack,
		type Pack,
		type PackNote,
		type PackSort,
		type World
	} from '$lib/api/packs';
	import { t } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);
	const canConfigure = $derived(server.permissions.includes('CONFIG'));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));
	const running = $derived(servers.isRunning(server.id));

	/** The pack whose note is open, and the note dialog's own open flag. */
	let noting = $state.raw<Pack | null>(null);
	let noteOpen = $state(false);

	function openNote(pack: Pack) {
		noting = pack;
		noteOpen = true;
	}

	/** Keeps the row's mark in step without asking the server for the list again. */
	function noted(note: PackNote | null) {
		const id = noting?.uuid;
		if (!id) return;
		packs = (packs ?? []).map((one) => (one.uuid === id ? { ...one, note } : one));
	}

	let packs = $state.raw<Pack[] | null>(null);
	let missing = $state.raw<MissingPack[]>([]);
	let worlds = $state.raw<World[] | null>(null);
	let levelName = $state('');
	let loadFailed = $state(false);
	let busy = $state(false);

	let addonInput = $state.raw<HTMLInputElement | null>(null);
	let worldInput = $state.raw<HTMLInputElement | null>(null);

	/** An add-on waiting on the answer to "which world?". */
	let pending = $state.raw<File | null>(null);
	let target = $state('');

	// A Bedrock server unpacks dozens of its own packs, which would bury the one
	// somebody added, so they are folded away until asked for.
	let showStock = $state(false);
	const stockCount = $derived((packs ?? []).filter((one) => one.stock).length);

	let query = $state('');
	const searching = $derived(query.trim().length > 0);

	// A pack is looked for by what the operator has to hand: its name, the
	// folder it sits in, the id a server log names it by, or the note about it.
	const matches = $derived.by(() => {
		const needle = query.trim().toLowerCase();
		if (!needle) return packs ?? [];
		return (packs ?? []).filter((one) =>
			[one.name, one.path, one.uuid ?? '', one.note?.text ?? ''].some((field) =>
				field.toLowerCase().includes(needle)
			)
		);
	});

	const shown = $derived(matches.filter((one) => showStock || !one.stock));
	/** Matches folded away with the stock packs, so a search never looks empty
	 * when the answer is only hidden. */
	const stockMatches = $derived(searching ? matches.length - shown.length : 0);

	async function refresh(id: string) {
		try {
			const [found, keeps] = await Promise.all([listPacks(id), listWorlds(id)]);
			if (server.id !== id) return;
			packs = found.packs;
			missing = found.missing;
			worlds = keeps.worlds;
			levelName = keeps.level_name;
			loadFailed = false;
		} catch {
			if (server.id !== id) return;
			loadFailed = true;
		}
	}

	async function drop(pack: Pack) {
		const ok = await confirm({
			title: t('content.removeTitle', { name: pack.name }),
			description: pack.stock
				? `${t('content.removeStock')} ${t('content.removeBody')}`
				: t('content.removeBody'),
			confirmLabel: t('content.remove'),
			destructive: true
		});
		if (!ok) return;

		busy = true;
		try {
			const gone = await removePack(server.id, pack.path);
			// A world the panel could not tidy still names the pack, which is the
			// very warning this was meant to prevent, so it is said out loud.
			if (gone.skipped.length) {
				toast.warning(t('content.removed', { name: gone.removed }), {
					description: t('content.removedSkipped', { worlds: gone.skipped.join(', ') })
				});
			} else {
				toast.success(t('content.removed', { name: gone.removed }), {
					description: gone.worlds.length
						? t('content.removedFrom', { worlds: gone.worlds.join(', ') })
						: t('content.removedNowhere')
				});
			}
			await refresh(server.id);
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function forget(gone: MissingPack) {
		busy = true;
		try {
			await forgetPack(server.id, gone.uuid);
			toast.success(t('content.forgotten', { uuid: gone.uuid, world: levelName }));
			await refresh(server.id);
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	$effect(() => {
		const id = server.id;
		packs = null;
		missing = [];
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

	{#if missing.length}
		<Alert.Root variant="destructive">
			<TriangleAlertIcon />
			<Alert.Title>{t('content.missingTitle')}</Alert.Title>
			<Alert.Description class="space-y-3">
				<p>{t('content.missingBody')}</p>
				<ul class="space-y-1">
					{#each missing as gone (gone.uuid)}
						<li class="flex flex-wrap items-center gap-2">
							<code class="font-mono text-[11px]">{gone.uuid}</code>
							<span class="text-xs text-muted-foreground">
								{sortLabel(gone.sort)}
								{#if gone.version.length}
									· {t('content.version', { version: gone.version.join('.') })}
								{/if}
							</span>
							<Button
								variant="outline"
								size="sm"
								class="ml-auto"
								disabled={busy}
								onclick={() => forget(gone)}
							>
								{t('content.forget')}
							</Button>
						</li>
					{/each}
				</ul>
			</Alert.Description>
		</Alert.Root>
	{/if}

	<section class="space-y-3">
		<div class="flex flex-wrap items-center justify-between gap-3">
			<h2 class="text-sm font-medium">{t('content.packs')}</h2>
			<div class="flex flex-1 flex-wrap items-center justify-end gap-3">
				<InputGroup.Root class="max-w-xs">
					<InputGroup.Addon><SearchIcon /></InputGroup.Addon>
					<InputGroup.Input
						bind:value={query}
						placeholder={t('content.search')}
						aria-label={t('content.search')}
					/>
				</InputGroup.Root>
				{#if stockCount}
					<div class="flex items-center gap-2">
						<Switch id="show-stock" bind:checked={showStock} />
						<Label for="show-stock" class="text-xs font-normal text-muted-foreground">
							{showStock ? t('content.showStock') : t('content.stockHidden', { count: stockCount })}
						</Label>
					</div>
				{/if}
			</div>
		</div>
		{#if stockMatches > 0}
			<p class="text-xs text-muted-foreground">
				{t('content.stockMatches', { count: String(stockMatches) })}
			</p>
		{/if}
		{#if !packs}
			<Skeleton class="h-40 rounded-lg" />
		{:else if shown.length === 0}
			<Empty.Root class="rounded-lg border border-dashed py-12">
				<Empty.Header>
					<Empty.Media variant="icon">
						{#if searching}<SearchIcon />{:else}<BoxIcon />{/if}
					</Empty.Media>
					<Empty.Title>
						{#if searching}
							{t('content.noMatch')}
						{:else}
							{stockCount ? t('content.onlyStock') : t('content.noPacks')}
						{/if}
					</Empty.Title>
					<Empty.Description>
						{#if searching}
							{t('content.noMatchHint')}
						{:else}
							{stockCount ? t('content.onlyStockHint') : t('content.noPacksHint')}
						{/if}
					</Empty.Description>
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
							<Table.Head class="w-12"></Table.Head>
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each shown as pack (pack.path)}
							<Table.Row>
								<Table.Cell>
									<div class="flex items-center gap-2">
										<span class="font-medium">{pack.name}</span>
										{#if pack.note}
											<Tooltip.Provider>
												<Tooltip.Root>
													<Tooltip.Trigger>
														<NotebookPenIcon
															class="size-3.5 text-muted-foreground"
															aria-label={t('content.notesHas')}
														/>
													</Tooltip.Trigger>
													<Tooltip.Content>{t('content.notesHas')}</Tooltip.Content>
												</Tooltip.Root>
											</Tooltip.Provider>
										{/if}
										{#if pack.stock}
											<Badge variant="outline" class="text-muted-foreground">
												{t('content.stock')}
											</Badge>
										{/if}
									</div>
									<div class="text-xs text-muted-foreground">
										{sortLabel(pack.sort)}
										{#if pack.version.length}
											· {t('content.version', { version: pack.version.join('.') })}
										{/if}
									</div>
									{#if pack.uuid}
										<div class="mt-1 truncate font-mono text-[11px] text-muted-foreground/70">
											{pack.uuid}
										</div>
									{/if}
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
								<Table.Cell class="text-right">
									<DropdownMenu.Root>
										<DropdownMenu.Trigger>
											{#snippet child({ props })}
												<Button
													variant="ghost"
													size="icon-sm"
													aria-label={t('content.remove')}
													disabled={busy}
													{...props}
												>
													<EllipsisIcon />
												</Button>
											{/snippet}
										</DropdownMenu.Trigger>
										<DropdownMenu.Content align="end">
											{#if pack.uuid}
												<DropdownMenu.Item onSelect={() => openNote(pack)}>
													<NotebookPenIcon />
													{t('content.notes')}
												</DropdownMenu.Item>
											{/if}
											<DropdownMenu.Item variant="destructive" onSelect={() => drop(pack)}>
												<TrashIcon />
												{t('content.remove')}
											</DropdownMenu.Item>
										</DropdownMenu.Content>
									</DropdownMenu.Root>
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

<PackNoteDialog
	serverId={server.id}
	pack={noting}
	bind:open={noteOpen}
	canEdit={canConfigure}
	canRun={canCommand}
	{running}
	onsaved={noted}
/>

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
