<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import ExternalLinkIcon from '@lucide/svelte/icons/external-link';
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import SearchIcon from '@lucide/svelte/icons/search';
	import UploadIcon from '@lucide/svelte/icons/upload';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as RadioGroup from '$lib/components/ui/radio-group/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { errorMessage } from '$lib/api/servers';
	import {
		installFromUmod,
		pluginUpdates,
		removePlugin,
		rustMods,
		searchUmod,
		switchFramework,
		togglePlugin,
		uploadPlugin,
		type RustFramework,
		type RustMods,
		type UmodPlugin
	} from '$lib/api/rust';
	import { servers } from '$lib/servers.svelte';
	import { t } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);
	const canConfigure = $derived(server.permissions.includes('CONFIG'));
	const live = $derived(servers.isLive(server.id));
	const working = $derived(server.flags.updating || server.flags.installing);

	let mods = $state.raw<RustMods | null>(null);
	let loadFailed = $state(false);
	let chosen = $state<'none' | RustFramework>('none');
	let busy = $state(false);
	let latest = $state.raw<Record<string, string> | null>(null);
	let checking = $state(false);
	let uploadInput = $state<HTMLInputElement | null>(null);

	let query = $state('');
	let results = $state.raw<UmodPlugin[] | null>(null);
	let page = $state(1);
	let lastPage = $state(1);
	let searching = $state(false);

	const current = $derived<'none' | RustFramework>(mods?.framework ?? 'none');
	const installedNames = $derived(new Set(mods?.plugins.map((plugin) => plugin.name)));
	const configFolder = $derived(current === 'carbon' ? 'carbon/configs' : 'oxide/config');

	async function load() {
		loadFailed = false;
		try {
			mods = await rustMods(server.id);
			chosen = mods.framework ?? 'none';
		} catch {
			loadFailed = true;
		}
	}

	$effect(() => {
		void server.id;
		// A framework change finishes in the background, so look again once it has.
		void working;
		load();
	});

	async function act(work: () => Promise<unknown>, done?: string) {
		busy = true;
		try {
			await work();
			if (done) toast.success(done);
			await load();
		} catch (err) {
			toast.error(t('plugins.installed.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function apply() {
		busy = true;
		try {
			await switchFramework(server.id, chosen === 'none' ? null : chosen);
			toast(t('plugins.framework.started'), { description: t('plugins.framework.startedHint') });
		} catch (err) {
			toast.error(t('plugins.framework.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function upload(event: Event) {
		const input = event.currentTarget as HTMLInputElement;
		const file = input.files?.[0];
		input.value = '';
		if (!file) return;
		await act(
			() => uploadPlugin(server.id, file),
			t('plugins.installed.uploaded', { name: file.name })
		);
	}

	async function check() {
		checking = true;
		try {
			latest = (await pluginUpdates(server.id)).latest;
			const behind = mods?.plugins.filter((plugin) => isBehind(plugin.name, plugin.version));
			if (!behind?.length) toast.success(t('plugins.installed.upToDate'));
		} catch (err) {
			toast.error(t('plugins.installed.failed'), { description: errorMessage(err) });
		} finally {
			checking = false;
		}
	}

	const isBehind = (name: string, version: string | null) =>
		!!latest?.[name] && !!version && latest[name] !== version;

	async function search(more = false) {
		searching = true;
		try {
			const next = more ? page + 1 : 1;
			const found = await searchUmod(server.id, query.trim(), next);
			results = more && results ? [...results, ...found.plugins] : found.plugins;
			page = found.page;
			lastPage = found.last_page;
		} catch (err) {
			toast.error(t('plugins.installed.failed'), { description: errorMessage(err) });
		} finally {
			searching = false;
		}
	}

	const frameworkLabel = (key: 'none' | RustFramework) => t(`plugins.framework.${key}`);
	const frameworkHint = (key: 'none' | RustFramework) => t(`plugins.framework.${key}Hint`);
</script>

<svelte:head><title>{t('plugins.title')} · {server.name} · Crustation</title></svelte:head>

<div class="page space-y-6 py-4">
	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('plugins.installed.failed')}</Alert.Description>
		</Alert.Root>
	{:else if !mods}
		<Skeleton class="h-64 rounded-lg" />
	{:else}
		<div class="grid gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] xl:items-start">
			<section class="rounded-lg border bg-card">
				<div class="flex flex-wrap items-baseline justify-between gap-2 border-b px-5 py-4">
					<h2 class="text-base font-semibold tracking-tight">{t('plugins.framework.title')}</h2>
					{#if mods.version}
						<span class="truncate font-mono text-xs text-muted-foreground">{mods.version}</span>
					{/if}
				</div>
				<div class="space-y-4 px-5 py-4">
					<RadioGroup.Root bind:value={chosen} class="gap-3" disabled={!canConfigure}>
						{#each ['none', 'oxide', 'carbon'] as const as key (key)}
							<Label class="items-start font-normal">
								<RadioGroup.Item value={key} class="mt-0.5" />
								<span class="space-y-0.5">
									<span class="flex items-center gap-2 font-medium">
										{frameworkLabel(key)}
										{#if key === current && key !== 'none'}
											<Badge variant="secondary">{t('plugins.installed.enabled')}</Badge>
										{/if}
									</span>
									<span class="block text-xs text-muted-foreground">{frameworkHint(key)}</span>
								</span>
							</Label>
						{/each}
					</RadioGroup.Root>
					{#if canConfigure}
						<div class="flex flex-wrap items-center gap-3">
							<Button disabled={busy || working || live || chosen === current} onclick={apply}>
								{#if working}<Spinner />{/if}
								{working ? t('plugins.framework.working') : t('plugins.framework.apply')}
							</Button>
							{#if live && chosen !== current}
								<span class="text-xs text-muted-foreground">{t('plugins.framework.running')}</span>
							{/if}
						</div>
					{/if}
				</div>
			</section>

			{#if mods.framework}
				<section class="rounded-lg border bg-card">
					<div class="flex flex-wrap items-center justify-between gap-2 border-b px-5 py-3">
						<h2 class="text-base font-semibold tracking-tight">{t('plugins.installed.title')}</h2>
						<div class="flex items-center gap-2">
							<Button
								variant="outline"
								size="sm"
								disabled={checking || !mods.plugins.length}
								onclick={check}
							>
								{#if checking}<Spinner />{:else}<RefreshCwIcon />{/if}
								{checking ? t('plugins.installed.checking') : t('plugins.installed.check')}
							</Button>
							<Button size="sm" disabled={busy} onclick={() => uploadInput?.click()}>
								<UploadIcon />
								{t('plugins.installed.upload')}
							</Button>
							<input
								bind:this={uploadInput}
								type="file"
								accept=".cs"
								class="hidden"
								onchange={upload}
							/>
						</div>
					</div>

					{#if mods.plugins.length === 0}
						<p class="px-5 py-8 text-center text-sm text-muted-foreground">
							{t('plugins.installed.empty')}
						</p>
					{:else}
						<Table.Root>
							<Table.Body>
								{#each mods.plugins as plugin (plugin.file)}
									<Table.Row>
										<Table.Cell class="py-2">
											<span class="font-medium">{plugin.title ?? plugin.name}</span>
											<span class="block text-xs text-muted-foreground">
												{[plugin.version && `v${plugin.version}`, plugin.author]
													.filter(Boolean)
													.join(' · ')}
											</span>
										</Table.Cell>
										<Table.Cell class="py-2 text-right">
											{#if isBehind(plugin.name, plugin.version)}
												<Button
													variant="outline"
													size="sm"
													disabled={busy}
													onclick={() =>
														act(
															() => installFromUmod(server.id, plugin.name),
															t('plugins.installed.updated', { name: plugin.name })
														)}
												>
													<DownloadIcon />
													{t('plugins.installed.update', { version: latest?.[plugin.name] ?? '' })}
												</Button>
											{/if}
										</Table.Cell>
										<Table.Cell class="w-16 py-2">
											<Switch
												checked={plugin.enabled}
												disabled={busy}
												aria-label={t('plugins.installed.enabled')}
												onCheckedChange={(on) =>
													act(() => togglePlugin(server.id, plugin.file, on))}
											/>
										</Table.Cell>
										<Table.Cell class="w-10 py-2">
											<Button
												variant="ghost"
												size="icon-sm"
												aria-label={t('plugins.installed.remove', { name: plugin.name })}
												disabled={busy}
												onclick={() =>
													act(
														() => removePlugin(server.id, plugin.file),
														t('plugins.installed.removed', { name: plugin.name })
													)}
											>
												<XIcon />
											</Button>
										</Table.Cell>
									</Table.Row>
								{/each}
							</Table.Body>
						</Table.Root>
					{/if}
					<p class="border-t px-5 py-3 text-xs text-muted-foreground">
						{t('plugins.installed.hint', { folder: configFolder })}
					</p>
				</section>
			{/if}
		</div>

		{#if mods.framework}
			<section class="rounded-lg border bg-card">
				<div class="border-b px-5 py-4">
					<h2 class="text-base font-semibold tracking-tight">{t('plugins.umod.title')}</h2>
				</div>
				<div class="space-y-4 px-5 py-4">
					<form
						class="flex max-w-xl gap-2"
						onsubmit={(event) => {
							event.preventDefault();
							search();
						}}
					>
						<Input
							bind:value={query}
							placeholder={t('plugins.umod.search')}
							aria-label={t('plugins.umod.search')}
						/>
						<Button type="submit" disabled={searching}>
							{#if searching}<Spinner />{:else}<SearchIcon />{/if}
							{t('plugins.umod.search')}
						</Button>
					</form>

					{#if results?.length === 0}
						<p class="text-sm text-muted-foreground">{t('plugins.umod.empty')}</p>
					{:else if results}
						<ul class="grid gap-3 md:grid-cols-2 2xl:grid-cols-3">
							{#each results as plugin (plugin.name)}
								<li class="flex flex-col gap-2 rounded-lg border p-4">
									<div class="flex items-start justify-between gap-2">
										<div class="min-w-0">
											<a
												href={plugin.url}
												target="_blank"
												rel="noreferrer"
												class="inline-flex items-center gap-1 font-medium hover:underline"
											>
												{plugin.title}
												<ExternalLinkIcon class="size-3 text-muted-foreground" />
											</a>
											<p class="text-xs text-muted-foreground">
												{plugin.author} · v{plugin.version} ·
												{t('plugins.umod.downloads', {
													count: plugin.downloads.toLocaleString()
												})}
											</p>
										</div>
										{#if installedNames.has(plugin.name)}
											<Badge variant="secondary">{t('plugins.umod.installed')}</Badge>
										{:else}
											<Button
												size="sm"
												disabled={busy}
												onclick={() =>
													act(
														() => installFromUmod(server.id, plugin.name),
														t('plugins.umod.installedToast', { name: plugin.title })
													)}
											>
												<DownloadIcon />
												{t('plugins.umod.install')}
											</Button>
										{/if}
									</div>
									<p class="text-sm text-muted-foreground">{plugin.description}</p>
								</li>
							{/each}
						</ul>
						{#if page < lastPage}
							<Button variant="outline" disabled={searching} onclick={() => search(true)}>
								{t('plugins.umod.more')}
							</Button>
						{/if}
					{/if}
				</div>
			</section>
		{/if}
	{/if}
</div>
