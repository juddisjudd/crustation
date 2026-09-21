<script lang="ts">
	import PlayIcon from '@lucide/svelte/icons/play';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Textarea } from '$lib/components/ui/textarea/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import { errorMessage, sendCommand } from '$lib/api/servers';
	import {
		getPackNote,
		savePackNote,
		type NoteCommand,
		type Pack,
		type PackNote,
		type Suggestion
	} from '$lib/api/packs';
	import { t } from '$lib/i18n/index.svelte';

	let {
		serverId,
		pack,
		open = $bindable(false),
		canEdit,
		canRun,
		running,
		onsaved
	}: {
		serverId: string;
		pack: Pack | null;
		open: boolean;
		canEdit: boolean;
		canRun: boolean;
		running: boolean;
		onsaved?: (note: PackNote | null) => void;
	} = $props();

	let text = $state('');
	let commands = $state<NoteCommand[]>([]);
	let suggestions = $state.raw<Suggestion[]>([]);
	let loading = $state(false);
	let saving = $state(false);

	// Spelled out rather than built from the kind: the message keys are typed.
	const KINDS = {
		function: 'content.notesKindFunction',
		scriptevent: 'content.notesKindScriptevent',
		command: 'content.notesKindCommand',
		setting: 'content.notesKindSetting'
	} as const;

	const grouped = $derived(
		(Object.keys(KINDS) as (keyof typeof KINDS)[])
			.map((kind) => ({ kind, rows: suggestions.filter((one) => one.kind === kind) }))
			.filter((group) => group.rows.length > 0)
	);

	// Reloaded each time it opens, since the pack on disk may have changed.
	$effect(() => {
		const id = pack?.uuid;
		if (!open || !id) return;
		let cancelled = false;
		loading = true;
		getPackNote(serverId, id)
			.then((found) => {
				if (cancelled) return;
				text = found.text;
				commands = found.commands.map((one) => ({ ...one }));
				suggestions = found.suggestions;
			})
			.catch((err) => {
				if (!cancelled) toast.error(t('content.failed'), { description: errorMessage(err) });
			})
			.finally(() => {
				if (!cancelled) loading = false;
			});
		return () => {
			cancelled = true;
		};
	});

	function add(from?: Suggestion) {
		commands = [...commands, { label: from?.label ?? '', command: from?.command ?? '' }];
	}

	function drop(at: number) {
		commands = commands.filter((_, index) => index !== at);
	}

	async function save() {
		if (!pack?.uuid) return;
		saving = true;
		try {
			const kept = await savePackNote(serverId, pack.uuid, { text, commands });
			const held = kept.text.length > 0 || kept.commands.length > 0;
			onsaved?.(held ? kept : null);
			toast.success(t('content.notesSaved'));
			open = false;
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		} finally {
			saving = false;
		}
	}

	async function run(one: NoteCommand) {
		try {
			await sendCommand(serverId, one.command);
			toast.success(t('content.notesRan', { name: one.label || one.command }));
		} catch (err) {
			toast.error(t('content.failed'), { description: errorMessage(err) });
		}
	}
</script>

<Dialog.Root bind:open>
	<Dialog.Content class="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
		<Dialog.Header>
			<Dialog.Title>{t('content.notesFor', { name: pack?.name ?? '' })}</Dialog.Title>
			<Dialog.Description>{t('content.notesBody')}</Dialog.Description>
		</Dialog.Header>

		{#if loading}
			<Skeleton class="h-40 rounded-lg" />
		{:else}
			<div class="space-y-5">
				<Textarea
					bind:value={text}
					rows={4}
					disabled={!canEdit}
					placeholder={t('content.notesPlaceholder')}
				/>

				<section class="space-y-2">
					<div>
						<h3 class="text-sm font-medium">{t('content.notesCommands')}</h3>
						<p class="text-xs text-muted-foreground">{t('content.notesCommandsBody')}</p>
					</div>

					{#each commands as one, at (at)}
						<div class="flex items-center gap-2">
							<Input
								bind:value={one.label}
								disabled={!canEdit}
								class="w-40"
								aria-label={t('content.notesLabel')}
								placeholder={t('content.notesLabel')}
							/>
							<Input
								bind:value={one.command}
								disabled={!canEdit}
								class="flex-1 font-mono text-xs"
								aria-label={t('content.notesCommand')}
								placeholder={t('content.notesCommand')}
							/>
							{#if canRun}
								<Button
									variant="outline"
									size="icon-sm"
									disabled={!running || !one.command.trim()}
									aria-label={t('content.notesRun')}
									onclick={() => run(one)}
								>
									<PlayIcon />
								</Button>
							{/if}
							{#if canEdit}
								<Button
									variant="ghost"
									size="icon-sm"
									aria-label={t('content.notesRemoveCommand', { name: one.label })}
									onclick={() => drop(at)}
								>
									<XIcon />
								</Button>
							{/if}
						</div>
					{/each}

					{#if canEdit}
						<Button variant="outline" size="sm" onclick={() => add()}>
							<PlusIcon />
							{t('content.notesAddCommand')}
						</Button>
					{/if}
					{#if commands.length > 0}
						<p class="text-xs text-muted-foreground">{t('content.notesPlayerWarning')}</p>
					{/if}
				</section>

				<section class="space-y-3">
					<div>
						<h3 class="text-sm font-medium">{t('content.notesFound')}</h3>
						<p class="text-xs text-muted-foreground">{t('content.notesFoundBody')}</p>
					</div>

					{#if grouped.length === 0}
						<p class="text-xs text-muted-foreground">{t('content.notesNothingFound')}</p>
					{/if}

					{#each grouped as group (group.kind)}
						<div class="space-y-1">
							<Label class="text-xs text-muted-foreground">
								{t(KINDS[group.kind])}
							</Label>
							{#if group.kind === 'setting'}
								<p class="text-xs text-muted-foreground">{t('content.notesKindSettingBody')}</p>
							{/if}
							{#each group.rows as row (row.label + (row.command ?? ''))}
								<div class="flex items-center gap-2 rounded-md border px-2 py-1.5">
									<div class="min-w-0 flex-1">
										<div class="truncate text-sm">{row.label}</div>
										{#if row.command}
											<div class="truncate font-mono text-[11px] text-muted-foreground">
												{row.command}
											</div>
										{:else if row.detail}
											<div class="truncate text-[11px] text-muted-foreground">{row.detail}</div>
										{/if}
									</div>
									{#if canEdit && row.command}
										<Button variant="ghost" size="sm" onclick={() => add(row)}>
											{t('content.notesAdd')}
										</Button>
									{/if}
								</div>
							{/each}
						</div>
					{/each}
				</section>
			</div>
		{/if}

		<Dialog.Footer>
			<Button variant="outline" onclick={() => (open = false)}>{t('content.cancel')}</Button>
			{#if canEdit}
				<Button onclick={save} disabled={saving || loading}>{t('content.notesSave')}</Button>
			{/if}
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
