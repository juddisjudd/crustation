<script lang="ts">
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Field from '$lib/components/ui/field/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import SettingsCard from '$lib/components/settings-card.svelte';
	import { confirmWith } from '$lib/components/confirm/confirm.svelte';
	import { ApiError } from '$lib/api/client';
	import { deleteServer, errorMessage } from '$lib/api/servers';
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

	const canConfigure = $derived(server.permissions.includes('CONFIG'));
	const running = $derived(servers.isRunning(server.id));

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

	const groups = $derived.by(() => {
		const out: { name: string; items: ServerProperty[] }[] = [];
		for (const entry of loaded?.settings ?? []) {
			let group = out.find((candidate) => candidate.name === entry.group);
			if (!group) out.push((group = { name: entry.group, items: [] }));
			group.items.push(entry);
		}
		return out;
	});

	const editable = $derived((loaded?.other ?? []).filter((entry) => !entry.managed));
	const managed = $derived((loaded?.other ?? []).filter((entry) => entry.managed));
	const changed = $derived(Object.keys(edits));

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

	$effect(() => {
		// Re-reads when the page moves to another server.
		void server.id;
		load();
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

<div class="mx-auto w-full max-w-6xl px-4 py-6 md:px-8">
	<div
		class="sticky top-12 z-9 -mx-4 flex flex-wrap items-end justify-between gap-4 bg-background/90 px-4 py-2 backdrop-blur md:-mx-8 md:px-8"
	>
		<div>
			<h1 class="text-base font-semibold tracking-tight">{t('settings.title')}</h1>
			<p class="mt-1 text-sm text-muted-foreground">{t('settings.description')}</p>
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
				<Button size="sm" onclick={save} disabled={saving}>
					{#if saving}<Spinner class="size-4" />{/if}
					{saving ? t('settings.saving') : t('settings.save')}
				</Button>
			</div>
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
		{#if !loaded.exists}
			<Alert.Root class="mt-6">
				<Alert.Description>{t('settings.missing')}</Alert.Description>
			</Alert.Root>
		{/if}

		<div class="mt-6 space-y-6">
			{#each groups as group (group.name)}
				<section class="rounded-lg border bg-card p-6">
					<h2 class="mb-4 text-sm font-medium">{group.name}</h2>
					<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
						{#each group.items as entry (entry.key)}
							{@const invalid = !!fieldErrors[entry.key]}
							<Field.Field data-invalid={invalid}>
								{#if entry.type === 'flag'}
									<Label class="font-normal">
										<Checkbox
											checked={shown(entry) === 'true'}
											onCheckedChange={(on) => (edits[entry.key] = on ? 'true' : 'false')}
										/>
										{entry.label}
									</Label>
								{:else}
									<Field.Label for={entry.key}>{entry.label}</Field.Label>
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
								{:else if entry.type === 'number'}
									<Input
										id={entry.key}
										type="number"
										min={entry.min}
										max={entry.max}
										value={shown(entry)}
										oninput={(event) => (edits[entry.key] = event.currentTarget.value)}
										class="tabular-nums"
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

			<section class="rounded-lg border bg-card p-6">
				<div class="mb-4 space-y-1.5">
					<h2 class="text-sm font-medium">{t('settings.other.title')}</h2>
					<p class="text-sm text-muted-foreground">{t('settings.other.description')}</p>
				</div>

				{#if !editable.length && !managed.length}
					<p class="text-sm text-muted-foreground">{t('settings.other.empty')}</p>
				{:else}
					<div class="space-y-3">
						{#each editable as entry (entry.key)}
							{@const invalid = !!fieldErrors[entry.key]}
							<Field.Field orientation="responsive" data-invalid={invalid}>
								<Field.Label for={entry.key} class="font-mono text-xs">{entry.key}</Field.Label>
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
			</section>

			{#if canConfigure}
				<SettingsCard
					destructive
					title={t('settings.danger.title')}
					description={t('settings.danger.description')}
				>
					<p class="text-sm text-muted-foreground">{t('settings.danger.filesHint')}</p>

					{#snippet hint()}
						{#if running}{t('settings.danger.running')}{/if}
					{/snippet}
					{#snippet action()}
						<Button variant="destructive" size="sm" disabled={running || deleting} onclick={erase}>
							{#if deleting}<Spinner class="size-4" />{:else}<Trash2Icon />{/if}
							{t('settings.danger.button')}
						</Button>
					{/snippet}
				</SettingsCard>
			{/if}
		</div>
	{/if}
</div>
