<script lang="ts">
	import PlusIcon from '@lucide/svelte/icons/plus';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { errorMessage } from '$lib/api/servers';
	import {
		addSharedBan,
		isSteamId,
		liftSharedBan,
		setSharedBans,
		sharedBans,
		type SharedBan
	} from '$lib/api/rust';
	import type { Server } from '$lib/api/types';
	import { dateTime } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	const LENGTHS = [
		['forever', ''],
		['h1', '1h'],
		['h12', '12h'],
		['d1', '1d'],
		['d7', '7d'],
		['d30', '30d']
	] as const;
	const lengthLabel = (key: string) => t(`players.rust.lengths.${key}` as MessageKey);
	const canConfigure = $derived(server.permissions.includes('CONFIG'));

	let enabled = $state(false);
	let canEdit = $state(false);
	let bans = $state.raw<SharedBan[]>([]);
	let busy = $state(false);
	let steamId = $state('');
	let name = $state('');
	let reason = $state('');
	let length = $state('');
	let mute = $state(false);

	async function load() {
		try {
			const found = await sharedBans(server.id);
			enabled = found.enabled;
			canEdit = found.can_edit;
			bans = found.bans;
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		}
	}

	$effect(() => {
		void server.id;
		load();
	});

	async function act(work: () => Promise<unknown>) {
		busy = true;
		try {
			await work();
			await load();
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function add() {
		if (!isSteamId(steamId)) {
			toast.error(t('players.rust.badId'));
			return;
		}
		await act(() =>
			addSharedBan({
				steam_id: steamId.trim(),
				name: name.trim() || undefined,
				reason: reason.trim() || undefined,
				duration: length || undefined,
				mute
			})
		);
		steamId = '';
		name = '';
		reason = '';
	}

	const expiry = (expires: number | null) =>
		expires === null
			? t('players.rust.lengths.forever')
			: t('players.rust.until', { when: dateTime(new Date(expires * 1000).toISOString()) });
</script>

<div class="mt-4 space-y-4">
	<div class="flex items-start gap-3">
		<Switch
			id="use-shared"
			checked={enabled}
			disabled={busy || !canConfigure}
			onCheckedChange={(on) => act(() => setSharedBans(server.id, on))}
		/>
		<div class="space-y-0.5">
			<Label for="use-shared">{t('players.rust.useShared')}</Label>
			<p class="text-xs text-muted-foreground">{t('players.rust.sharedHint')}</p>
		</div>
	</div>

	{#if canEdit}
		<form
			class="flex flex-wrap items-end gap-2"
			onsubmit={(event) => {
				event.preventDefault();
				add();
			}}
		>
			<Input
				bind:value={steamId}
				placeholder={t('players.rust.steamId')}
				aria-label={t('players.rust.steamId')}
				class="w-48 font-mono"
				inputmode="numeric"
			/>
			<Input
				bind:value={name}
				placeholder={t('players.add.name')}
				aria-label={t('players.add.name')}
				class="w-36"
			/>
			<Input
				bind:value={reason}
				placeholder={t('players.add.reason')}
				aria-label={t('players.add.reason')}
				class="w-40"
			/>
			<Select.Root type="single" bind:value={length}>
				<Select.Trigger class="w-32" aria-label={t('players.rust.length')}>
					{lengthLabel(LENGTHS.find(([, code]) => code === length)?.[0] ?? 'forever')}
				</Select.Trigger>
				<Select.Content>
					{#each LENGTHS as [key, code] (key)}
						<Select.Item value={code} label={lengthLabel(key)} />
					{/each}
				</Select.Content>
			</Select.Root>
			<Label class="h-9 font-normal">
				<Checkbox bind:checked={mute} />
				{t('players.rust.muteInstead')}
			</Label>
			<Button type="submit" disabled={busy || !steamId.trim()}>
				<PlusIcon />
				{t('players.add.button')}
			</Button>
		</form>
	{/if}

	{#if bans.length}
		<div class="overflow-x-auto rounded-lg border">
			<Table.Root>
				<Table.Body>
					{#each bans as ban (ban.steam_id)}
						<Table.Row>
							<Table.Cell>
								<span class="font-medium">{ban.name || ban.steam_id}</span>
								{#if ban.mute}
									<Badge variant="secondary" class="ml-1">{t('players.rust.muted')}</Badge>
								{/if}
								<span class="block font-mono text-xs text-muted-foreground">{ban.steam_id}</span>
							</Table.Cell>
							<Table.Cell class="max-w-56 truncate text-xs text-muted-foreground">
								{ban.reason}
							</Table.Cell>
							<Table.Cell class="text-xs whitespace-nowrap text-muted-foreground">
								{expiry(ban.expires_at)}
							</Table.Cell>
							<Table.Cell class="w-10">
								{#if canEdit}
									<Button
										variant="ghost"
										size="icon-sm"
										aria-label={t('players.remove', { name: ban.name || ban.steam_id })}
										disabled={busy}
										onclick={() => act(() => liftSharedBan(ban.steam_id))}
									>
										<XIcon />
									</Button>
								{/if}
							</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
		</div>
	{:else}
		<p class="text-sm text-muted-foreground">{t('players.empty')}</p>
	{/if}
</div>
