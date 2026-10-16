<script lang="ts">
	import CalendarClockIcon from '@lucide/svelte/icons/calendar-clock';
	import CrownIcon from '@lucide/svelte/icons/crown';
	import EraserIcon from '@lucide/svelte/icons/eraser';
	import GlobeIcon from '@lucide/svelte/icons/globe';
	import HammerIcon from '@lucide/svelte/icons/hammer';
	import HeartPulseIcon from '@lucide/svelte/icons/heart-pulse';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import MegaphoneIcon from '@lucide/svelte/icons/megaphone';
	import PackageIcon from '@lucide/svelte/icons/package';
	import PickaxeIcon from '@lucide/svelte/icons/pickaxe';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import ScaleIcon from '@lucide/svelte/icons/scale';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ServerIcon from '@lucide/svelte/icons/server';
	import Settings2Icon from '@lucide/svelte/icons/settings-2';
	import SlidersHorizontalIcon from '@lucide/svelte/icons/sliders-horizontal';
	import SmartphoneIcon from '@lucide/svelte/icons/smartphone';
	import SparklesIcon from '@lucide/svelte/icons/sparkles';
	import SwordsIcon from '@lucide/svelte/icons/swords';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import TrendingUpIcon from '@lucide/svelte/icons/trending-up';
	import UsersIcon from '@lucide/svelte/icons/users';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Field from '$lib/components/ui/field/index.js';
	import * as RadioGroup from '$lib/components/ui/radio-group/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import SettingsCard from '$lib/components/settings-card.svelte';
	import RustTags from '$lib/components/servers/rust-tags.svelte';
	import { confirm, confirmWith } from '$lib/components/confirm/confirm.svelte';
	import { ApiError } from '$lib/api/client';
	import { deleteServer, errorMessage } from '$lib/api/servers';
	import { wipeRust, type RustWipe } from '$lib/api/rust';
	import { servers } from '$lib/servers.svelte';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import {
		readProperties,
		writeProperties,
		type OtherProperty,
		type ServerProperties,
		type ServerProperty
	} from '$lib/api/properties';
	import { t, plural } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);

	let loaded = $state.raw<ServerProperties | null>(null);
	let loadFailed = $state(false);
	// Only the keys touched since the last load, so a save sends nothing else.
	let edits = $state<Record<string, string>>({});
	let saving = $state(false);
	let fieldErrors = $state.raw<Record<string, string>>({});
	let deleting = $state(false);
	let wipeKind = $state<RustWipe>('map');
	let newSeed = $state(true);
	let wiping = $state(false);
	let query = $state('');
	let active = $state('');
	let toolbar = $state<HTMLElement>();
	let chips = $state<HTMLElement>();
	let jumpedAt = 0;

	const canConfigure = $derived(server.permissions.includes('CONFIG'));
	// Rust keeps its settings on the start command rather than in a file.
	const rust = $derived(server.kind === 'rust');
	const enshrouded = $derived(server.kind === 'enshrouded');
	// Enshrouded and Dragonwilds keep a settings file they rewrite while running.
	const gameFile = $derived(enshrouded || server.kind === 'dragonwilds');
	const running = $derived(servers.isLive(server.id));
	const locked = $derived(!!loaded?.stopped_only && running);

	async function erase() {
		const { confirmed, checked } = await confirmWith({
			title: t('settings.danger.confirmTitle', { name: server.name }),
			description: t('settings.danger.confirmBody'),
			confirmLabel: t('settings.danger.button'),
			destructive: true,
			checkbox: t('settings.danger.files')
		});
		if (!confirmed) return;

		deleting = true;
		try {
			await deleteServer(server.id, checked);
			toast.success(t('settings.danger.done', { name: server.name }));
			await servers.refresh();
			await goto(resolve('/'));
		} catch (err) {
			toast.error(t('settings.danger.failed'), { description: errorMessage(err) });
			deleting = false;
		}
	}

	async function wipe() {
		const confirmed = await confirm({
			title: t('settings.wipe.confirmTitle', { name: server.name }),
			description: t('settings.wipe.confirmBody'),
			confirmLabel: t('settings.wipe.button'),
			destructive: true
		});
		if (!confirmed) return;

		wiping = true;
		try {
			const result = await wipeRust(server.id, wipeKind, newSeed);
			toast.success(plural('settings.wipe.done', result.deleted.length), {
				description:
					result.seed === null ? undefined : t('settings.wipe.seed', { seed: result.seed })
			});
			if (result.seed !== null) await load();
		} catch (err) {
			toast.error(t('settings.wipe.failed'), { description: errorMessage(err) });
		} finally {
			wiping = false;
		}
	}

	const ICONS: Record<string, typeof GlobeIcon> = {
		World: GlobeIcon,
		Players: UsersIcon,
		Listing: MegaphoneIcon,
		Rules: ScaleIcon,
		Events: SparklesIcon,
		Wipe: CalendarClockIcon,
		Creative: HammerIcon,
		'Rust+': SmartphoneIcon,
		Server: ServerIcon,
		Survival: HeartPulseIcon,
		Enemies: SwordsIcon,
		Resources: PickaxeIcon,
		Experience: TrendingUpIcon,
		Owner: CrownIcon,
		Access: KeyRoundIcon,
		'Resource pack': PackageIcon
	};

	const needle = $derived(query.trim().toLowerCase());
	const matches = (...texts: (string | null | undefined)[]) =>
		!needle || texts.some((text) => text?.toLowerCase().includes(needle));
	const countEdits = (entries: { key: string }[]) =>
		entries.filter((entry) => entry.key in edits).length;

	const groups = $derived.by(() => {
		const out: { id: string; name: string; items: ServerProperty[]; changes: number }[] = [];
		for (const entry of loaded?.settings ?? []) {
			let group = out.find((candidate) => candidate.name === entry.group);
			if (!group) {
				const id = entry.group.toLowerCase().replace(/[^a-z0-9]+/g, '-');
				out.push((group = { id, name: entry.group, items: [], changes: 0 }));
			}
			group.items.push(entry);
		}
		for (const group of out) group.changes = countEdits(group.items);
		return out
			.map((group) => ({
				...group,
				items: group.items.filter((entry) => matches(entry.label, entry.key, entry.help))
			}))
			.filter((group) => group.items.length);
	});

	const editable = $derived(
		(loaded?.other ?? []).filter((entry) => !entry.managed && matches(entry.key, entry.value))
	);
	const managed = $derived(
		(loaded?.other ?? []).filter((entry) => entry.managed && matches(entry.key, entry.value))
	);
	const showOther = $derived(!needle || editable.length + managed.length > 0);
	const showWipe = $derived(
		canConfigure && rust && matches(t('settings.wipe.title'), t('settings.wipe.description'))
	);
	const showDelete = $derived(
		canConfigure && matches(t('settings.danger.title'), t('settings.danger.button'))
	);
	const changed = $derived(Object.keys(edits));

	const sections = $derived([
		...groups.map((group) => ({
			id: group.id,
			label: group.name,
			icon: ICONS[group.name] ?? Settings2Icon,
			changes: group.changes,
			danger: false
		})),
		...(showOther
			? [
					{
						id: 'other',
						label: t('settings.nav.other'),
						icon: SlidersHorizontalIcon,
						changes: countEdits(loaded?.other ?? []),
						danger: false
					}
				]
			: []),
		...(showWipe
			? [
					{
						id: 'wipe-now',
						label: t('settings.wipe.title'),
						icon: EraserIcon,
						changes: 0,
						danger: true
					}
				]
			: []),
		...(showDelete
			? [
					{
						id: 'delete',
						label: t('settings.danger.button'),
						icon: Trash2Icon,
						changes: 0,
						danger: true
					}
				]
			: [])
	]);

	/** What the file holds, or the server's own default when it says nothing. */
	const shown = (entry: ServerProperty) =>
		edits[entry.key] ?? (entry.set ? entry.value : entry.default);

	const shownOther = (entry: OtherProperty) => edits[entry.key] ?? entry.value;

	function optionLabel(value: string) {
		return value
			.replace(/^minecraft:/, '')
			.replace(/[_-]/g, ' ')
			.replace(/^./, (first) => first.toUpperCase());
	}

	function spy() {
		// A click's smooth scroll passes other sections on the way; let it land first.
		if (Date.now() - jumpedAt < 1000) return;
		const line = Math.max((toolbar?.getBoundingClientRect().bottom ?? 0) + 32, innerHeight * 0.3);
		const found = [...document.querySelectorAll<HTMLElement>('[data-section]')];
		let current = found[0]?.dataset.section ?? '';
		for (const element of found) {
			if (element.getBoundingClientRect().top <= line) current = element.dataset.section ?? '';
		}
		const atBottom =
			window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 4;
		active = atBottom ? (found.at(-1)?.dataset.section ?? current) : current;
	}

	function jump(id: string) {
		active = id;
		jumpedAt = Date.now();
		document.querySelector(`[data-section="${id}"]`)?.scrollIntoView({
			behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth',
			block: 'start'
		});
	}

	$effect(() => {
		void sections;
		requestAnimationFrame(spy);
	});

	$effect(() => {
		void active;
		const chip = chips?.querySelector<HTMLElement>('[aria-current]');
		if (!chips || !chip) return;
		chips.scrollTo({
			left: chip.offsetLeft - chips.offsetLeft - (chips.clientWidth - chip.offsetWidth) / 2
		});
	});

	async function load() {
		loadFailed = false;
		try {
			loaded = await readProperties(server.id);
			edits = {};
			fieldErrors = {};
		} catch {
			loadFailed = true;
		}
	}

	// The server object is replaced on any state change; only a new id should drop unsaved edits.
	const serverId = $derived(server.id);

	$effect(() => {
		void serverId;
		untrack(load);
	});

	async function save() {
		saving = true;
		fieldErrors = {};
		try {
			const result = await writeProperties(server.id, { ...edits });
			toast.success(plural('settings.saved', changed.length), {
				description: result.restart_required ? t('settings.restart') : undefined
			});
			await load();
		} catch (err) {
			if (err instanceof ApiError) {
				// The API keys them as settings.<name>; the form knows them by name.
				fieldErrors = Object.fromEntries(
					Object.entries(err.fields).map(([key, message]) => [
						key.replace(/^settings\./, ''),
						message
					])
				);
			}
			toast.error(t('settings.failed'), { description: errorMessage(err) });
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head><title>{t('nav.tabs.settings')} · {server.name} · Crustation</title></svelte:head>

<svelte:window onscroll={spy} onresize={spy} />

{#snippet heading(icon: typeof GlobeIcon, title: string, changes: number, description?: string)}
	{@const Icon = icon}
	<header class="flex items-center gap-3 border-b bg-muted/50 px-5 py-3">
		<span class="flex size-8 shrink-0 items-center justify-center rounded-md border bg-background">
			<Icon class="size-4" />
		</span>
		<div class="min-w-0">
			<h2 class="text-base font-semibold tracking-tight">{title}</h2>
			{#if description}<p class="text-sm text-muted-foreground">{description}</p>{/if}
		</div>
		{#if changes}
			<span
				class="ml-auto shrink-0 rounded-full bg-primary px-2 py-0.5 text-xs text-primary-foreground tabular-nums"
			>
				{plural('settings.changes', changes)}
			</span>
		{/if}
	</header>
{/snippet}

{#snippet edited(key: string)}
	{#if key in edits}
		<span class="size-1.5 shrink-0 rounded-full bg-primary" title={t('settings.edited')}></span>
	{/if}
{/snippet}

<div class="page py-4">
	<div>
		<h1 class="text-base font-semibold tracking-tight">
			{rust || gameFile ? t('settings.rust.title') : t('settings.title')}
		</h1>
		<p class="mt-1 text-sm text-muted-foreground">
			{rust
				? t('settings.rust.description')
				: gameFile
					? enshrouded
						? t('settings.game.enshrouded')
						: t('settings.game.dragonwilds')
					: t('settings.description')}
		</p>
	</div>

	<div
		bind:this={toolbar}
		class="sticky top-12 z-9 -mx-4 mt-2 space-y-2 border-b bg-background/90 px-4 py-2 backdrop-blur md:-mx-6 md:px-6 2xl:-mx-8 2xl:px-8"
	>
		<div class="flex flex-wrap items-center justify-between gap-2">
			<div class="relative w-full sm:w-72">
				<SearchIcon
					class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
				/>
				<Input
					type="search"
					bind:value={query}
					placeholder={t('settings.filter')}
					aria-label={t('settings.filter')}
					onkeydown={(event) => event.key === 'Escape' && (query = '')}
					class="pr-8 pl-8 [&::-webkit-search-cancel-button]:hidden"
				/>
				{#if query}
					<button
						type="button"
						class="absolute top-1/2 right-1.5 flex size-6 -translate-y-1/2 items-center justify-center rounded text-muted-foreground hover:text-foreground"
						aria-label={t('settings.clearFilter')}
						onclick={() => (query = '')}
					>
						<XIcon class="size-3.5" />
					</button>
				{/if}
			</div>
			{#if changed.length}
				<div class="flex items-center gap-2">
					<span class="text-sm text-muted-foreground tabular-nums">
						{plural('settings.changes', changed.length)}
					</span>
					<Button variant="outline" size="sm" onclick={() => (edits = {})} disabled={saving}>
						<RotateCcwIcon />
						{t('settings.discard')}
					</Button>
					<Button size="sm" onclick={save} disabled={saving || locked}>
						{#if saving}<Spinner class="size-4" />{/if}
						{saving ? t('settings.saving') : t('settings.save')}
					</Button>
				</div>
			{/if}
		</div>
		{#if loaded && sections.length}
			<nav
				bind:this={chips}
				aria-label={t('settings.nav.label')}
				class="-mx-1 flex [scrollbar-width:none] gap-1 overflow-x-auto px-1 lg:hidden"
			>
				{#each sections as section (section.id)}
					{@const Icon = section.icon}
					<button
						type="button"
						aria-current={active === section.id ? 'true' : undefined}
						onclick={() => jump(section.id)}
						class={[
							'inline-flex shrink-0 items-center gap-1.5 rounded-full border px-3 py-1 text-xs transition-colors',
							active === section.id
								? 'border-primary bg-primary text-primary-foreground'
								: 'text-muted-foreground hover:text-foreground',
							section.danger && active !== section.id && 'text-destructive'
						]}
					>
						<Icon class="size-3.5" />
						{section.label}
						{#if section.changes}<span class="size-1.5 rounded-full bg-current"></span>{/if}
					</button>
				{/each}
			</nav>
		{/if}
	</div>

	{#if loadFailed}
		<Alert.Root variant="destructive" class="mt-6">
			<Alert.Description>{t('settings.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !loaded}
		<div class="mt-6 space-y-4">
			{#each [0, 1, 2] as i (i)}
				<Skeleton class="h-32 rounded-lg" />
			{/each}
		</div>
	{:else}
		{#if locked}
			<Alert.Root class="mt-6">
				<Alert.Description>{t('settings.game.locked')}</Alert.Description>
			</Alert.Root>
		{/if}
		{#if !loaded.exists}
			<Alert.Root class="mt-6">
				<Alert.Description>
					{gameFile ? t('settings.game.missing') : t('settings.missing')}
				</Alert.Description>
			</Alert.Root>
		{/if}

		<div class="mt-6 lg:grid lg:grid-cols-[12rem_minmax(0,1fr)] lg:gap-8">
			<nav aria-label={t('settings.nav.label')} class="hidden lg:block">
				<ul class="sticky top-32 space-y-0.5">
					{#each sections as section, index (section.id)}
						{@const Icon = section.icon}
						{#if section.danger && !sections[index - 1]?.danger}
							<li class="my-2 border-t" aria-hidden="true"></li>
						{/if}
						<li>
							<button
								type="button"
								aria-current={active === section.id ? 'true' : undefined}
								onclick={() => jump(section.id)}
								class={[
									'flex w-full items-center gap-2 rounded-md px-2.5 py-1.5 text-left text-sm transition-colors',
									active === section.id
										? 'bg-accent font-medium text-foreground'
										: 'text-muted-foreground hover:bg-accent/60 hover:text-foreground',
									section.danger && 'text-destructive hover:text-destructive'
								]}
							>
								<Icon class="size-4 shrink-0" />
								<span class="min-w-0 flex-1 truncate">{section.label}</span>
								{#if section.changes}
									<span class="text-xs text-foreground tabular-nums">{section.changes}</span>
								{/if}
							</button>
						</li>
					{/each}
				</ul>
			</nav>

			<div class="min-w-0 space-y-6">
				{#if needle && !sections.length}
					<p class="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
						{t('settings.noMatch', { query: query.trim() })}
					</p>
				{/if}

				{#each groups as group (group.id)}
					<section
						data-section={group.id}
						class="scroll-mt-40 overflow-hidden rounded-lg border bg-card lg:scroll-mt-32"
					>
						{@render heading(ICONS[group.name] ?? Settings2Icon, group.name, group.changes)}
						<div class="grid gap-x-6 gap-y-5 p-5 sm:grid-cols-2 2xl:grid-cols-3">
							{#each group.items as entry (entry.key)}
								{@const invalid = !!fieldErrors[entry.key]}
								<Field.Field
									data-invalid={invalid}
									class={entry.key === 'server.tags' ? 'sm:col-span-full' : undefined}
								>
									{#if entry.type === 'flag'}
										<Label class="font-normal">
											<Checkbox
												checked={shown(entry) === 'true'}
												onCheckedChange={(on) => (edits[entry.key] = on ? 'true' : 'false')}
											/>
											{entry.label}
											{@render edited(entry.key)}
										</Label>
									{:else}
										<Field.Label for={entry.key}>
											{entry.label}
											{@render edited(entry.key)}
										</Field.Label>
									{/if}

									{#if entry.type === 'choice'}
										<Select.Root
											type="single"
											value={shown(entry)}
											onValueChange={(picked) => (edits[entry.key] = picked)}
										>
											<Select.Trigger id={entry.key} class="w-full">
												{optionLabel(shown(entry))}
											</Select.Trigger>
											<Select.Content>
												{#each entry.options as option (option)}
													<Select.Item value={option} label={optionLabel(option)} />
												{/each}
											</Select.Content>
										</Select.Root>
									{:else if entry.type === 'number' || entry.type === 'decimal'}
										<Input
											id={entry.key}
											type="number"
											min={entry.min}
											max={entry.max}
											step={entry.type === 'decimal' ? 'any' : undefined}
											value={shown(entry)}
											oninput={(event) => (edits[entry.key] = event.currentTarget.value)}
											class="tabular-nums"
										/>
									{:else if entry.key === 'server.tags'}
										<RustTags
											id={entry.key}
											value={shown(entry)}
											onchange={(next) => (edits[entry.key] = next)}
										/>
									{:else if entry.type === 'text'}
										<Input
											id={entry.key}
											value={shown(entry)}
											placeholder={entry.default}
											oninput={(event) => (edits[entry.key] = event.currentTarget.value)}
										/>
									{/if}

									{#if invalid}
										<Field.Error>{fieldErrors[entry.key]}</Field.Error>
									{:else if entry.help}
										<Field.Description>{entry.help}</Field.Description>
									{:else if !entry.set}
										<Field.Description>{t('settings.unset')}</Field.Description>
									{/if}
								</Field.Field>
							{/each}
						</div>
					</section>
				{/each}

				{#if showOther}
					<section
						data-section="other"
						class="scroll-mt-40 overflow-hidden rounded-lg border bg-card lg:scroll-mt-32"
					>
						{@render heading(
							SlidersHorizontalIcon,
							rust
								? t('settings.rust.otherTitle')
								: gameFile
									? t('settings.game.otherTitle')
									: t('settings.other.title'),
							countEdits(loaded.other),
							rust ? t('settings.rust.otherDescription') : undefined
						)}
						<div class="p-5">
							{#if !editable.length && !managed.length}
								<p class="text-sm text-muted-foreground">
									{rust ? t('settings.rust.otherEmpty') : t('settings.other.empty')}
								</p>
							{:else}
								<div class="space-y-3">
									{#each editable as entry (entry.key)}
										{@const invalid = !!fieldErrors[entry.key]}
										<Field.Field orientation="responsive" data-invalid={invalid}>
											<Field.Label for={entry.key} class="font-mono text-xs">
												{entry.key}
												{@render edited(entry.key)}
											</Field.Label>
											<Field.Content>
												<Input
													id={entry.key}
													value={shownOther(entry)}
													oninput={(event) => (edits[entry.key] = event.currentTarget.value)}
													class="font-mono text-xs"
												/>
												{#if invalid}<Field.Error>{fieldErrors[entry.key]}</Field.Error>{/if}
											</Field.Content>
										</Field.Field>
									{/each}

									{#each managed as entry (entry.key)}
										<Field.Field orientation="responsive">
											<Field.Label class="font-mono text-xs text-muted-foreground">
												{entry.key}
											</Field.Label>
											<Field.Content>
												<Input value={entry.value} disabled class="font-mono text-xs" />
												<Field.Description>{t('settings.other.managed')}</Field.Description>
											</Field.Content>
										</Field.Field>
									{/each}
								</div>
							{/if}
						</div>
					</section>
				{/if}

				{#if showWipe}
					<div data-section="wipe-now" class="scroll-mt-40 lg:scroll-mt-32">
						<SettingsCard
							destructive
							title={t('settings.wipe.title')}
							description={t('settings.wipe.description')}
						>
							<RadioGroup.Root bind:value={wipeKind} class="gap-3">
								{#each ['map', 'blueprints', 'full'] as const as kind (kind)}
									<Label class="items-start font-normal">
										<RadioGroup.Item value={kind} class="mt-0.5" />
										<span class="space-y-0.5">
											<span class="block font-medium">{t(`settings.wipe.${kind}`)}</span>
											<span class="block text-xs text-muted-foreground"
												>{t(`settings.wipe.${kind}Hint`)}</span
											>
										</span>
									</Label>
								{/each}
							</RadioGroup.Root>
							{#if wipeKind !== 'blueprints'}
								<Label class="font-normal">
									<Checkbox bind:checked={newSeed} />
									{t('settings.wipe.newSeed')}
								</Label>
							{/if}

							{#snippet hint()}
								{#if running}{t('settings.wipe.running')}{/if}
							{/snippet}
							{#snippet action()}
								<Button variant="destructive" size="sm" disabled={running || wiping} onclick={wipe}>
									{#if wiping}<Spinner class="size-4" />{:else}<EraserIcon />{/if}
									{t('settings.wipe.button')}
								</Button>
							{/snippet}
						</SettingsCard>
					</div>
				{/if}

				{#if showDelete}
					<div data-section="delete" class="scroll-mt-40 lg:scroll-mt-32">
						<SettingsCard
							destructive
							title={t('settings.danger.title')}
							description={t('settings.danger.description')}
						>
							{#snippet hint()}
								{#if running}{t('settings.danger.running')}{/if}
							{/snippet}
							{#snippet action()}
								<Button
									variant="destructive"
									size="sm"
									disabled={running || deleting}
									onclick={erase}
								>
									{#if deleting}<Spinner class="size-4" />{:else}<Trash2Icon />{/if}
									{t('settings.danger.button')}
								</Button>
							{/snippet}
						</SettingsCard>
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>
