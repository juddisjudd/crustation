<script lang="ts">
	import CheckCircleIcon from '@lucide/svelte/icons/circle-check';
	import AlertTriangleIcon from '@lucide/svelte/icons/triangle-alert';
	import UploadIcon from '@lucide/svelte/icons/upload';
	import { untrack } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import PageHeader from '$lib/components/page-header.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Progress } from '$lib/components/ui/progress/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Field from '$lib/components/ui/field/index.js';
	import * as RadioGroup from '$lib/components/ui/radio-group/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { ApiError } from '$lib/api/client';
	import { errorMessage } from '$lib/api/servers';
	import {
		archiveRoots,
		createServer,
		listProperties,
		listVersions,
		uploadArchive,
		type ArchiveRoot,
		type CreateSource,
		type InstallProgress,
		type KnownProperty,
		type NewServer,
		type ProviderVersion
	} from '$lib/api/create';
	import type { ServerKind } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { socket } from '$lib/realtime/socket.svelte';
	import { kindLabel } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { data } = $props();

	let tab = $state<'install' | 'import'>('install');
	// A starting point only; the operator owns the choice after that.
	let providerId = $state(untrack(() => data.providers[0]?.id ?? ''));
	let versions = $state.raw<ProviderVersion[]>([]);
	let versionsLoading = $state(false);
	let versionsError = $state('');
	let version = $state('');
	let showUnstable = $state(false);

	let importMode = $state<'zip' | 'folder' | 'url'>('zip');
	let importKind = $state<ServerKind>('minecraft_java');
	let uploadId = $state('');
	let uploadName = $state('');
	let uploading = $state(false);
	let roots = $state.raw<ArchiveRoot[]>([]);
	let internalPath = $state('');
	let folderPath = $state('');
	let downloadUrl = $state('');
	let executable = $state('');

	let name = $state('');
	let port = $state<number | null>(null);
	let host = $state('0.0.0.0');
	let minMemory = $state(1024);
	let maxMemory = $state(4096);
	let javaBinary = $state('');
	let javaFlags = $state('');
	let autostart = $state(false);
	let eula = $state(false);
	let advanced = $state(false);

	let catalogue = $state.raw<KnownProperty[]>([]);
	let catalogueFailed = $state(false);
	// Only the settings the operator touched; the rest keep the server's defaults.
	let settings = $state<Record<string, string>>({});
	let showSettings = $state(false);

	let submitting = $state(false);
	let fieldErrors = $state.raw<Record<string, string>>({});

	let createdId = $state('');
	let createdName = $state('');
	let progress = $state.raw<InstallProgress | null>(null);

	const provider = $derived(data.providers.find((entry) => entry.id === providerId));
	const kind = $derived<ServerKind>(
		tab === 'install' ? (provider?.kind ?? 'minecraft_java') : importKind
	);
	const needsJava = $derived(kind === 'minecraft_java');
	const defaultPort = $derived(
		tab === 'install'
			? (provider?.default_port ?? 25565)
			: importKind === 'minecraft_bedrock'
				? 19132
				: 25565
	);
	const shown = $derived(showUnstable ? versions : versions.filter((entry) => entry.stable));
	const versionLabel = $derived(
		shown.find((entry) => entry.id === version)?.label ?? t('create.source.versionPlaceholder')
	);
	const javaLabel = $derived(
		data.java.find((runtime) => runtime.path === javaBinary)?.version ??
			t('create.details.javaAuto')
	);

	const groups = $derived.by(() => {
		const out: { name: string; items: KnownProperty[] }[] = [];
		for (const entry of catalogue) {
			let group = out.find((candidate) => candidate.name === entry.group);
			if (!group) out.push((group = { name: entry.group, items: [] }));
			group.items.push(entry);
		}
		return out;
	});

	// `minecraft:large_biomes` reads better as `Large biomes`.
	function optionLabel(value: string) {
		return value
			.replace(/^minecraft:/, '')
			.replace(/[_-]/g, ' ')
			.replace(/^./, (first) => first.toUpperCase());
	}

	const valueOf = (entry: KnownProperty) => settings[entry.key] ?? entry.default;

	const ready = $derived.by(() => {
		if (!name.trim() || !eula) return false;
		if (tab === 'install') return !!version;
		if (importMode === 'zip') return !!uploadId;
		if (importMode === 'folder') return !!folderPath.trim();
		return !!downloadUrl.trim();
	});

	$effect(() => {
		const wanted = providerId;
		if (tab !== 'install' || !wanted) return;

		versionsLoading = true;
		versionsError = '';
		listVersions(wanted)
			.then((list) => {
				if (providerId !== wanted) return;
				versions = list;
				version = (list.find((entry) => entry.stable) ?? list[0])?.id ?? '';
			})
			.catch((err) => {
				if (providerId !== wanted) return;
				versions = [];
				version = '';
				versionsError = errorMessage(err);
			})
			.finally(() => {
				if (providerId === wanted) versionsLoading = false;
			});
	});

	$effect(() => {
		const wanted = kind;
		catalogueFailed = false;
		listProperties(wanted)
			.then((list) => {
				if (kind !== wanted) return;
				catalogue = list;
				// Java keys mean nothing to Bedrock, so start over on a change.
				settings = {};
			})
			.catch(() => {
				if (kind !== wanted) return;
				catalogue = [];
				catalogueFailed = true;
			});
	});

	$effect(() => {
		const id = createdId;
		if (!id) return;
		const unsubscribe = socket.subscribe([`server:${id}`]);
		const off = socket.on<InstallProgress>(`server:${id}`, 'install', (data) => {
			progress = data;
		});
		return () => {
			off();
			unsubscribe();
		};
	});

	async function onFile(event: Event & { currentTarget: HTMLInputElement }) {
		const file = event.currentTarget.files?.[0];
		if (!file) return;

		uploading = true;
		uploadId = '';
		roots = [];
		internalPath = '';
		try {
			const result = await uploadArchive(file);
			uploadId = result.upload_id;
			uploadName = file.name;
			roots = await archiveRoots(result.upload_id).catch(() => []);
			// One obvious candidate is the answer; several need the operator.
			if (roots.length === 1) internalPath = roots[0].path;
		} catch (err) {
			toast.error(t('create.uploadFailed'), { description: errorMessage(err) });
		} finally {
			uploading = false;
		}
	}

	function source(): CreateSource {
		if (tab === 'install') return { type: 'provider', provider: providerId, version };
		const chosen = executable.trim() || undefined;
		if (importMode === 'zip') {
			return {
				type: 'zip',
				kind: importKind,
				upload_id: uploadId,
				internal_path: internalPath,
				executable: chosen
			};
		}
		if (importMode === 'folder') {
			return { type: 'folder', kind: importKind, path: folderPath.trim(), executable: chosen };
		}
		return { type: 'url', kind: importKind, url: downloadUrl.trim(), executable: chosen };
	}

	async function submit() {
		fieldErrors = {};
		submitting = true;
		try {
			const body: NewServer = {
				name: name.trim(),
				host: host.trim() || '0.0.0.0',
				port: port ?? defaultPort,
				autostart,
				agree_to_eula: eula,
				source: source()
			};
			if (needsJava) {
				body.min_memory_mb = minMemory;
				body.max_memory_mb = maxMemory;
				body.java_binary = javaBinary || null;
				body.java_flags = javaFlags.trim();
			}
			if (Object.keys(settings).length) body.properties = settings;

			const created = await createServer(body);
			createdId = created.id;
			createdName = body.name;
			servers.refresh();
		} catch (err) {
			if (err instanceof ApiError) fieldErrors = err.fields;
			toast.error(t('create.failed'), { description: errorMessage(err) });
		} finally {
			submitting = false;
		}
	}

	function again() {
		createdId = '';
		progress = null;
		name = '';
		uploadId = '';
		uploadName = '';
		roots = [];
		internalPath = '';
		eula = false;
		settings = {};
		showSettings = false;
	}
</script>

<svelte:head><title>{t('create.title')} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-3xl px-4 py-8 md:px-8">
	{#if createdId}
		{@const phase = progress?.phase}
		{@const failed = phase === 'failed'}
		{@const done = phase === 'done'}
		<PageHeader title={t('create.progress.title', { name: createdName })} />

		<div class="mt-8 rounded-lg border bg-card p-6">
			<div class="flex items-center gap-3">
				{#if failed}
					<AlertTriangleIcon class="size-5 text-destructive" />
				{:else if done}
					<CheckCircleIcon class="size-5 text-success" />
				{:else}
					<Spinner class="size-5" />
				{/if}
				<div class="min-w-0 flex-1">
					<p class="font-medium">
						{phase ? t(`create.progress.${phase}` as MessageKey) : t('create.progress.waiting')}
					</p>
					{#if progress?.message && !done}
						<p class="mt-0.5 truncate text-sm text-muted-foreground">{progress.message}</p>
					{/if}
				</div>
			</div>

			{#if !failed}
				<Progress value={done ? 100 : (progress?.percent ?? 0)} class="mt-4" />
			{/if}

			<div class="mt-6 flex flex-wrap items-center gap-2">
				<Button onclick={() => goto(resolve(`/servers/${createdId}`))}>
					{t('create.progress.open')}
				</Button>
				<Button variant="outline" onclick={again}>{t('create.progress.again')}</Button>
				<span class="text-xs text-muted-foreground">{t('create.progress.leaveHint')}</span>
			</div>
		</div>
	{:else}
		<PageHeader title={t('create.title')} description={t('create.description')} />

		<form
			class="mt-8 space-y-6"
			onsubmit={(event) => {
				event.preventDefault();
				submit();
			}}
		>
			<section class="rounded-lg border bg-card p-6">
				<h2 class="text-base font-semibold tracking-tight">{t('create.source.title')}</h2>

				<Tabs.Root bind:value={tab} class="mt-4">
					<Tabs.List>
						<Tabs.Trigger value="install">{t('create.tabs.install')}</Tabs.Trigger>
						<Tabs.Trigger value="import">{t('create.tabs.import')}</Tabs.Trigger>
					</Tabs.List>

					<Tabs.Content value="install" class="mt-4 space-y-4">
						<p class="text-sm text-muted-foreground">{t('create.source.installDescription')}</p>

						<RadioGroup.Root bind:value={providerId} class="grid gap-3 sm:grid-cols-2">
							{#each data.providers as entry (entry.id)}
								<Label
									class="flex cursor-pointer items-start gap-3 rounded-lg border p-3 has-data-checked:border-primary"
								>
									<RadioGroup.Item value={entry.id} class="mt-0.5" />
									<span class="min-w-0 space-y-0.5">
										<span class="block font-medium">{entry.name}</span>
										<span class="block text-xs font-normal text-muted-foreground">
											{entry.summary}
										</span>
									</span>
								</Label>
							{/each}
						</RadioGroup.Root>

						<Field.Field>
							<Field.Label for="version">{t('create.source.version')}</Field.Label>
							{#if versionsLoading}
								<p class="text-sm text-muted-foreground">{t('create.source.loadingVersions')}</p>
							{:else if versionsError}
								<p class="text-sm text-destructive">
									{t('create.source.versionsFailed', { provider: provider?.name ?? providerId })}
								</p>
							{:else}
								<Select.Root type="single" bind:value={version}>
									<Select.Trigger id="version" class="w-full">{versionLabel}</Select.Trigger>
									<Select.Content>
										{#each shown as entry (entry.id)}
											<Select.Item value={entry.id} label={entry.label} />
										{:else}
											<Select.Item value="" label={t('create.source.noVersions')} disabled />
										{/each}
									</Select.Content>
								</Select.Root>
							{/if}
							<Label class="mt-1 font-normal text-muted-foreground">
								<Checkbox bind:checked={showUnstable} />
								{t('create.source.showUnstable')}
							</Label>
						</Field.Field>
					</Tabs.Content>

					<Tabs.Content value="import" class="mt-4 space-y-4">
						<p class="text-sm text-muted-foreground">{t('create.source.importDescription')}</p>

						<Field.Field>
							<Field.Label>{t('create.import.mode')}</Field.Label>
							<RadioGroup.Root bind:value={importMode} class="gap-2">
								<Label class="font-normal">
									<RadioGroup.Item value="zip" />
									{t('create.import.modeZip')}
								</Label>
								<Label class="font-normal">
									<RadioGroup.Item value="folder" disabled={!session.superuser} />
									{t('create.import.modeFolder')}
								</Label>
								<Label class="font-normal">
									<RadioGroup.Item value="url" />
									{t('create.import.modeUrl')}
								</Label>
							</RadioGroup.Root>
						</Field.Field>

						<Field.Field>
							<Field.Label>{t('create.import.kind')}</Field.Label>
							<RadioGroup.Root bind:value={importKind} class="gap-2">
								{#each ['minecraft_java', 'minecraft_bedrock'] as option (option)}
									<Label class="font-normal">
										<RadioGroup.Item value={option} />
										{kindLabel(option)}
									</Label>
								{/each}
							</RadioGroup.Root>
						</Field.Field>

						{#if importMode === 'zip'}
							<Field.Field>
								<Field.Label for="archive">{t('create.import.file')}</Field.Label>
								<div class="flex items-center gap-3">
									<input
										id="archive"
										type="file"
										accept=".zip,application/zip"
										onchange={onFile}
										class="block w-full text-sm file:mr-3 file:rounded-md file:border file:border-input file:bg-background file:px-3 file:py-1.5 file:text-sm file:font-medium"
									/>
									{#if uploading}<Spinner class="size-4 shrink-0" />{/if}
								</div>
								{#if uploadId}
									<Field.Description class="inline-flex items-center gap-1.5">
										<UploadIcon class="size-3.5" />
										{t('create.import.uploaded', { name: uploadName })}
									</Field.Description>
								{:else}
									<Field.Description>{t('create.import.uploadHint')}</Field.Description>
								{/if}
							</Field.Field>

							{#if uploadId}
								<Field.Field>
									<Field.Label>{t('create.import.root')}</Field.Label>
									<RadioGroup.Root bind:value={internalPath} class="gap-2">
										<Label class="font-normal">
											<RadioGroup.Item value="" />
											{t('create.import.rootTop')}
										</Label>
										{#each roots.filter((root) => root.path) as root (root.path)}
											<Label class="font-normal">
												<RadioGroup.Item value={root.path} />
												<span class="font-mono text-xs">{root.path}</span>
											</Label>
										{/each}
									</RadioGroup.Root>
									<Field.Description>
										{roots.length ? t('create.import.rootHint') : t('create.import.rootsEmpty')}
									</Field.Description>
								</Field.Field>
							{/if}
						{:else if importMode === 'folder'}
							<Field.Field data-invalid={!!fieldErrors['source.path']}>
								<Field.Label for="folder">{t('create.import.path')}</Field.Label>
								<Input
									id="folder"
									bind:value={folderPath}
									placeholder="/mnt/user/appdata/minecraft"
									class="font-mono"
								/>
								{#if fieldErrors['source.path']}
									<Field.Error>{fieldErrors['source.path']}</Field.Error>
								{:else}
									<Field.Description>
										{session.superuser
											? t('create.import.pathHint')
											: t('create.import.pathAdminOnly')}
									</Field.Description>
								{/if}
							</Field.Field>
						{:else}
							<Field.Field data-invalid={!!fieldErrors['source.url']}>
								<Field.Label for="url">{t('create.import.url')}</Field.Label>
								<Input
									id="url"
									bind:value={downloadUrl}
									placeholder="https://example.com/server.zip"
									class="font-mono"
								/>
								{#if fieldErrors['source.url']}
									<Field.Error>{fieldErrors['source.url']}</Field.Error>
								{:else}
									<Field.Description>{t('create.import.urlHint')}</Field.Description>
								{/if}
							</Field.Field>
						{/if}

						<Field.Field>
							<Field.Label for="executable">{t('create.import.executable')}</Field.Label>
							<Input
								id="executable"
								bind:value={executable}
								placeholder="server.jar"
								class="font-mono"
							/>
							<Field.Description>{t('create.import.executableHint')}</Field.Description>
						</Field.Field>
					</Tabs.Content>
				</Tabs.Root>
			</section>

			<section class="space-y-4 rounded-lg border bg-card p-6">
				<h2 class="text-base font-semibold tracking-tight">{t('create.details.title')}</h2>

				<div class="grid gap-4 sm:grid-cols-2">
					<Field.Field data-invalid={!!fieldErrors.name}>
						<Field.Label for="name">{t('create.details.name')}</Field.Label>
						<Input
							id="name"
							bind:value={name}
							placeholder={t('create.details.namePlaceholder')}
							required
						/>
						{#if fieldErrors.name}<Field.Error>{fieldErrors.name}</Field.Error>{/if}
					</Field.Field>

					<Field.Field data-invalid={!!fieldErrors.port}>
						<Field.Label for="port">{t('create.details.port')}</Field.Label>
						<Input
							id="port"
							type="number"
							min="1"
							max="65535"
							placeholder={String(defaultPort)}
							value={port ?? ''}
							oninput={(event) => {
								const raw = event.currentTarget.value;
								port = raw === '' ? null : Number(raw);
							}}
							class="tabular-nums"
						/>
						{#if fieldErrors.port}<Field.Error>{fieldErrors.port}</Field.Error>{/if}
					</Field.Field>
				</div>

				{#if needsJava}
					<div class="grid gap-4 sm:grid-cols-2">
						<Field.Field data-invalid={!!fieldErrors.min_memory_mb}>
							<Field.Label for="min-memory">{t('create.details.minMemory')}</Field.Label>
							<Input
								id="min-memory"
								type="number"
								min="128"
								step="128"
								bind:value={minMemory}
								class="tabular-nums"
							/>
							{#if fieldErrors.min_memory_mb}
								<Field.Error>{fieldErrors.min_memory_mb}</Field.Error>
							{/if}
						</Field.Field>

						<Field.Field>
							<Field.Label for="max-memory">{t('create.details.maxMemory')}</Field.Label>
							<Input
								id="max-memory"
								type="number"
								min="128"
								step="128"
								bind:value={maxMemory}
								class="tabular-nums"
							/>
						</Field.Field>
					</div>

					<Field.Field>
						<Field.Label for="java">{t('create.details.java')}</Field.Label>
						<Select.Root type="single" bind:value={javaBinary}>
							<Select.Trigger id="java" class="w-full">{javaLabel}</Select.Trigger>
							<Select.Content>
								<Select.Item value="" label={t('create.details.javaAuto')} />
								{#each data.java as runtime (runtime.path)}
									<Select.Item value={runtime.path} label={runtime.version} />
								{/each}
							</Select.Content>
						</Select.Root>
						{#if data.java.length === 0}
							<Field.Description class="text-warning">
								{t('create.details.javaNone')}
							</Field.Description>
						{/if}
					</Field.Field>
				{/if}

				<Label class="font-normal text-muted-foreground">
					<Checkbox bind:checked={advanced} />
					{t('create.details.advanced')}
				</Label>

				{#if advanced}
					<div class="space-y-4 border-t pt-4">
						<Field.Field>
							<Field.Label for="host">{t('create.details.host')}</Field.Label>
							<Input id="host" bind:value={host} class="font-mono" />
							<Field.Description>{t('create.details.hostHint')}</Field.Description>
						</Field.Field>

						{#if needsJava}
							<Field.Field>
								<Field.Label for="flags">{t('create.details.flags')}</Field.Label>
								<Input
									id="flags"
									bind:value={javaFlags}
									placeholder="-XX:+UseG1GC"
									class="font-mono"
								/>
								<Field.Description>{t('create.details.flagsHint')}</Field.Description>
							</Field.Field>
						{/if}

						<Field.Field orientation="horizontal">
							<Field.Label for="autostart">{t('create.details.autostart')}</Field.Label>
							<Switch id="autostart" bind:checked={autostart} />
						</Field.Field>
					</div>
				{/if}
			</section>

			{#if catalogue.length || catalogueFailed}
				<section class="space-y-4 rounded-lg border bg-card p-6">
					<div class="space-y-1.5">
						<h2 class="text-base font-semibold tracking-tight">{t('create.properties.title')}</h2>
						<p class="text-sm text-muted-foreground">{t('create.properties.description')}</p>
					</div>

					{#if catalogueFailed}
						<p class="text-sm text-destructive">{t('create.properties.failed')}</p>
					{:else}
						<Label class="font-normal text-muted-foreground">
							<Checkbox bind:checked={showSettings} />
							{t('create.properties.show')}
						</Label>

						{#if showSettings}
							<div class="space-y-6 border-t pt-4">
								{#each groups as group (group.name)}
									<fieldset class="space-y-4">
										<legend class="mb-3 text-sm font-medium">{group.name}</legend>
										<div class="grid gap-4 sm:grid-cols-2">
											{#each group.items as entry (entry.key)}
												{@const invalid = !!fieldErrors[`properties.${entry.key}`]}
												<Field.Field data-invalid={invalid}>
													{#if entry.type === 'flag'}
														<Label class="font-normal">
															<Checkbox
																checked={valueOf(entry) === 'true'}
																onCheckedChange={(on) =>
																	(settings[entry.key] = on ? 'true' : 'false')}
															/>
															{entry.label}
														</Label>
													{:else}
														<Field.Label for={entry.key}>{entry.label}</Field.Label>
													{/if}

													{#if entry.type === 'choice'}
														<Select.Root
															type="single"
															value={valueOf(entry)}
															onValueChange={(picked) => (settings[entry.key] = picked)}
														>
															<Select.Trigger id={entry.key} class="w-full">
																{optionLabel(valueOf(entry))}
															</Select.Trigger>
															<Select.Content>
																{#each entry.options as option (option)}
																	<Select.Item value={option} label={optionLabel(option)} />
																{/each}
															</Select.Content>
														</Select.Root>
													{:else if entry.type === 'number'}
														<Input
															id={entry.key}
															type="number"
															min={entry.min}
															max={entry.max}
															value={valueOf(entry)}
															oninput={(event) => (settings[entry.key] = event.currentTarget.value)}
															class="tabular-nums"
														/>
													{:else if entry.type === 'text'}
														<Input
															id={entry.key}
															value={valueOf(entry)}
															placeholder={entry.key === 'level-seed'
																? t('create.properties.random')
																: entry.default}
															oninput={(event) => (settings[entry.key] = event.currentTarget.value)}
														/>
													{/if}

													{#if invalid}
														<Field.Error>{fieldErrors[`properties.${entry.key}`]}</Field.Error>
													{:else if entry.help}
														<Field.Description>{entry.help}</Field.Description>
													{/if}
												</Field.Field>
											{/each}
										</div>
									</fieldset>
								{/each}
							</div>
						{/if}
					{/if}
				</section>
			{/if}

			<section class="rounded-lg border bg-card p-6">
				<Field.Field data-invalid={!!fieldErrors.agree_to_eula}>
					<Label class="font-normal">
						<Checkbox bind:checked={eula} required />
						{t('create.eula.label')}
					</Label>
					<Field.Description>
						<a
							href="https://www.minecraft.net/eula"
							target="_blank"
							rel="noreferrer"
							class="underline underline-offset-4"
						>
							{t('create.eula.link')}
						</a>
					</Field.Description>
					{#if fieldErrors.agree_to_eula}
						<Field.Error>{fieldErrors.agree_to_eula}</Field.Error>
					{/if}
				</Field.Field>
			</section>

			<div class="flex items-center justify-end gap-2">
				<Button variant="outline" href={resolve('/')}>{t('common.actions.cancel')}</Button>
				<Button type="submit" disabled={!ready || submitting || uploading}>
					{#if submitting}<Spinner class="size-4" />{/if}
					{submitting ? t('create.submitting') : t('create.submit')}
				</Button>
			</div>
		</form>
	{/if}
</div>
