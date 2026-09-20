<script lang="ts">
	import PlugIcon from '@lucide/svelte/icons/plug';
	import TrashIcon from '@lucide/svelte/icons/trash-2';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { errorMessage } from '$lib/api/servers';
	import { bridgeStatus, installBridge, removeBridge, type BridgeStatus } from '$lib/api/bridge';
	import { dateTime } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';

	interface Props {
		serverId: string;
		canEdit: boolean;
	}

	let { serverId, canEdit }: Props = $props();

	let link = $state.raw<BridgeStatus | null>(null);
	let url = $state('');
	let busy = $state(false);

	async function look() {
		try {
			const found = await bridgeStatus(serverId);
			link = found;
			if (!url) url = found.suggested_url;
		} catch {
			link = null;
		}
	}

	$effect(() => {
		const id = serverId;
		link = null;
		look();
		// While it is connected the only thing that changes is the clock, so a
		// slow poll is plenty.
		const timer = setInterval(() => serverId === id && look(), 5000);
		return () => clearInterval(timer);
	});

	async function add() {
		busy = true;
		try {
			const done = await installBridge(serverId, url.trim() || undefined);
			toast.success(t('addon.installed'), {
				description: done.world_missing
					? t('addon.noWorldYet')
					: done.beta_apis_turned_on
						? t('addon.betaTurnedOn')
						: t('addon.onRestart')
			});
			await look();
		} catch (err) {
			toast.error(t('addon.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function drop() {
		const ok = await confirm({
			title: t('addon.removeTitle'),
			description: t('addon.removeBody'),
			confirmLabel: t('addon.remove'),
			destructive: true
		});
		if (!ok) return;
		busy = true;
		try {
			await removeBridge(serverId);
			toast.success(t('addon.removed'));
			await look();
		} catch (err) {
			toast.error(t('addon.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}
</script>

{#if !link}
	<Skeleton class="h-32 rounded-lg" />
{:else}
	{@const bridge = link}
	<div class="space-y-3">
		<div class="flex flex-wrap items-center gap-2">
			{#if bridge.connected}
				<Badge class="bg-success text-background">{t('addon.connected')}</Badge>
			{:else if bridge.installed}
				<Badge variant="secondary">{t('addon.waiting')}</Badge>
			{:else}
				<Badge variant="outline">{t('addon.notInstalled')}</Badge>
			{/if}
			{#if bridge.last_seen}
				<span class="text-xs text-muted-foreground">
					{t('addon.lastSeen', { when: dateTime(bridge.last_seen) })}
				</span>
			{/if}
		</div>

		{#if bridge.installed && !bridge.connected}
			<Alert.Root>
				<Alert.Description>
					{bridge.beta_apis === false
						? t('addon.betaOff')
						: bridge.beta_apis === null
							? t('addon.noWorld', { world: bridge.world })
							: t('addon.quiet')}
				</Alert.Description>
			</Alert.Root>
		{/if}

		{#if canEdit}
			<div class="grid gap-2">
				<Label for="panel-url">{t('addon.url')}</Label>
				<Input id="panel-url" bind:value={url} spellcheck={false} class="font-mono text-xs" />
				<p class="text-xs text-muted-foreground">{t('addon.urlHint')}</p>
			</div>

			<div class="flex flex-wrap items-center gap-2">
				<Button size="sm" disabled={busy} onclick={add}>
					{#if busy}<Spinner class="size-4" />{:else}<PlugIcon />{/if}
					{bridge.installed ? t('addon.reinstall') : t('addon.install')}
				</Button>
				{#if bridge.installed}
					<Button size="sm" variant="ghost" disabled={busy} onclick={drop}>
						<TrashIcon />
						{t('addon.remove')}
					</Button>
				{/if}
			</div>
		{/if}
	</div>
{/if}
