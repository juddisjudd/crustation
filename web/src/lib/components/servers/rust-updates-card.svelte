<script lang="ts">
	import DownloadIcon from '@lucide/svelte/icons/download';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Card from '$lib/components/ui/card/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import { rustBuild, updateRust, type RustBuild } from '$lib/api/rust';
	import { errorMessage } from '$lib/api/servers';
	import type { Server } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	const WARNINGS = [
		['none', 0],
		['m1', 60],
		['m5', 300],
		['m15', 900]
	] as const;

	const live = $derived(servers.isLive(server.id));
	const busy = $derived(server.flags.installing || server.flags.updating);

	let build = $state.raw<RustBuild | null>(null);
	let checking = $state(true);
	let warn = $state('300');
	let starting = $state(false);

	const warningLabel = (key: string) => t(`server.updates.warnings.${key}` as MessageKey);
	const branchLabel = (branch: string) => (branch === '' ? 'release' : branch);

	$effect(() => {
		const id = server.id;
		void server.flags.updating;
		checking = true;
		rustBuild(id)
			.then((found) => (build = found))
			.catch(() => (build = null))
			.finally(() => (checking = false));
	});

	async function update() {
		starting = true;
		try {
			await updateRust(server.id, live ? Number(warn) : 0);
			toast.success(t('server.updates.started', { name: server.name }), {
				description: live ? t('server.updates.startedRunning') : undefined
			});
		} catch (err) {
			toast.error(t('server.updates.failed'), { description: errorMessage(err) });
		} finally {
			starting = false;
		}
	}
</script>

<Card.Root>
	<Card.Header>
		<Card.Title class="flex items-center gap-2">
			{t('server.updates.title')}
			{#if build?.update_available}
				<Badge variant="destructive">{t('server.updates.available')}</Badge>
			{:else if build?.latest}
				<Badge variant="secondary">{t('server.updates.upToDate')}</Badge>
			{/if}
		</Card.Title>
	</Card.Header>
	<Card.Content class="space-y-4">
		<dl class="grid grid-cols-2 gap-3 text-sm">
			<div>
				<dt class="text-muted-foreground">{t('server.updates.installed')}</dt>
				<dd class="mt-1 font-mono tabular-nums">{build?.installed ?? '—'}</dd>
			</div>
			<div>
				<dt class="text-muted-foreground">
					{t('server.updates.latest', { branch: branchLabel(build?.branch ?? '') })}
				</dt>
				<dd class="mt-1 font-mono tabular-nums">
					{#if checking}
						<span class="inline-flex items-center gap-1.5 font-sans text-muted-foreground">
							<Spinner class="size-3.5" />
							{t('server.updates.checking')}
						</span>
					{:else}
						{build?.latest ?? t('server.updates.unknown')}
					{/if}
				</dd>
			</div>
		</dl>
		<div class="flex flex-wrap items-center gap-2">
			{#if live}
				<Select.Root type="single" bind:value={warn}>
					<Select.Trigger class="w-36" aria-label={t('server.updates.warn')}>
						{warningLabel(WARNINGS.find(([, seconds]) => String(seconds) === warn)?.[0] ?? 'none')}
					</Select.Trigger>
					<Select.Content>
						{#each WARNINGS as [key, seconds] (key)}
							<Select.Item value={String(seconds)} label={warningLabel(key)} />
						{/each}
					</Select.Content>
				</Select.Root>
			{/if}
			<Button
				variant={build?.update_available ? 'default' : 'outline'}
				disabled={busy || starting}
				onclick={update}
			>
				{#if busy || starting}<Spinner />{:else}<DownloadIcon />{/if}
				{t('server.updates.button')}
			</Button>
		</div>
	</Card.Content>
</Card.Root>
