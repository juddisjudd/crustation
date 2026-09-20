<script lang="ts">
	import BanIcon from '@lucide/svelte/icons/ban';
	import BootIcon from '@lucide/svelte/icons/door-open';
	import GiftIcon from '@lucide/svelte/icons/gift';
	import MapPinIcon from '@lucide/svelte/icons/map-pin';
	import MessageIcon from '@lucide/svelte/icons/message-square';
	import MoreIcon from '@lucide/svelte/icons/ellipsis';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import ShieldOffIcon from '@lucide/svelte/icons/shield-off';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { errorMessage } from '$lib/api/servers';
	import { actOnPlayer, type PlayerAction } from '$lib/api/players';
	import ItemPicker from './item-picker.svelte';
	import { t } from '$lib/i18n/index.svelte';

	interface Props {
		serverId: string;
		edition: 'java' | 'bedrock';
		player: string;
		operator: boolean;
		banned: boolean;
		running: boolean;
		canManage: boolean;
		canCommand: boolean;
		/** Other people on now, to teleport towards. */
		others?: string[];
		onchange?: () => void;
	}

	let {
		serverId,
		edition,
		player,
		operator,
		banned,
		running,
		canManage,
		canCommand,
		others = [],
		onchange
	}: Props = $props();

	type Kind = 'kick' | 'ban' | 'give' | 'teleport' | 'whisper';
	let asking = $state<Kind | null>(null);
	let busy = $state(false);

	let reason = $state('');
	let item = $state('diamond');
	let count = $state(1);
	let message = $state('');
	let target = $state('');
	let x = $state('0');
	let y = $state('64');
	let z = $state('0');

	const ranks = $derived(
		edition === 'bedrock' ? ['visitor', 'member', 'operator'] : ['1', '2', '3', '4']
	);

	async function run(action: PlayerAction, note: string) {
		busy = true;
		try {
			const result = await actOnPlayer(serverId, action);
			toast.success(note, {
				description: result.restart_required
					? t('players.act.onRestart')
					: result.via === 'file'
						? t('players.act.viaFile')
						: undefined
			});
			asking = null;
			onchange?.();
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	function open(kind: Kind) {
		reason = '';
		message = '';
		target = others[0] ?? '';
		asking = kind;
	}

	const coordinates = $derived({ x: Number(x), y: Number(y), z: Number(z) });
	const placeOk = $derived(Object.values(coordinates).every((one) => Number.isFinite(one)));
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger>
		{#snippet child({ props })}
			<Button
				{...props}
				variant="ghost"
				size="icon-sm"
				aria-label={t('players.act.menu', { name: player })}
			>
				<MoreIcon />
			</Button>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content align="end" class="w-52">
		{#if canManage}
			{#if operator}
				<DropdownMenu.Item
					onSelect={() =>
						run({ action: 'deop', player }, t('players.act.deopped', { name: player }))}
				>
					<ShieldOffIcon />
					{t('players.act.deop')}
				</DropdownMenu.Item>
			{:else}
				<DropdownMenu.Item
					onSelect={() => run({ action: 'op', player }, t('players.act.opped', { name: player }))}
				>
					<ShieldIcon />
					{t('players.act.op')}
				</DropdownMenu.Item>
			{/if}

			<DropdownMenu.Sub>
				<DropdownMenu.SubTrigger>{t('players.act.rank')}</DropdownMenu.SubTrigger>
				<DropdownMenu.SubContent>
					{#each ranks as rank (rank)}
						<DropdownMenu.Item
							onSelect={() =>
								run(
									{ action: 'rank', player, rank },
									t('players.act.ranked', { name: player, rank })
								)}
						>
							{edition === 'bedrock'
								? t(`players.act.ranks.${rank}` as 'players.act.ranks.member')
								: t('players.act.level', { level: rank })}
						</DropdownMenu.Item>
					{/each}
				</DropdownMenu.SubContent>
			</DropdownMenu.Sub>

			<DropdownMenu.Separator />

			<DropdownMenu.Item disabled={!running} onSelect={() => open('kick')}>
				<BootIcon />
				{t('players.act.kick')}
			</DropdownMenu.Item>

			{#if edition === 'java'}
				{#if banned}
					<DropdownMenu.Item
						onSelect={() =>
							run({ action: 'pardon', player }, t('players.act.pardoned', { name: player }))}
					>
						<UndoIcon />
						{t('players.act.pardon')}
					</DropdownMenu.Item>
				{:else}
					<DropdownMenu.Item variant="destructive" onSelect={() => open('ban')}>
						<BanIcon />
						{t('players.act.ban')}
					</DropdownMenu.Item>
				{/if}
			{/if}
		{/if}

		{#if canCommand}
			<DropdownMenu.Separator />
			<DropdownMenu.Item disabled={!running} onSelect={() => open('give')}>
				<GiftIcon />
				{t('players.act.give')}
			</DropdownMenu.Item>
			<DropdownMenu.Item disabled={!running} onSelect={() => open('teleport')}>
				<MapPinIcon />
				{t('players.act.teleport')}
			</DropdownMenu.Item>
			<DropdownMenu.Item disabled={!running} onSelect={() => open('whisper')}>
				<MessageIcon />
				{t('players.act.whisper')}
			</DropdownMenu.Item>
		{/if}
	</DropdownMenu.Content>
</DropdownMenu.Root>

<Dialog.Root open={asking !== null} onOpenChange={(next) => !next && (asking = null)}>
	<Dialog.Content class="sm:max-w-md">
		{#if asking === 'kick' || asking === 'ban'}
			<Dialog.Header>
				<Dialog.Title>
					{asking === 'kick'
						? t('players.act.kickTitle', { name: player })
						: t('players.act.banTitle', { name: player })}
				</Dialog.Title>
				<Dialog.Description>{t('players.act.reasonHint')}</Dialog.Description>
			</Dialog.Header>
			<div class="grid gap-2">
				<Label for="reason">{t('players.add.reason')}</Label>
				<Input id="reason" bind:value={reason} placeholder={t('players.act.reasonPlaceholder')} />
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}
					>{t('common.actions.cancel')}</Button
				>
				<Button
					variant="destructive"
					disabled={busy}
					onclick={() =>
						asking === 'kick'
							? run(
									{ action: 'kick', player, reason: reason.trim() || undefined },
									t('players.act.kicked', { name: player })
								)
							: run(
									{ action: 'ban', player, reason: reason.trim() || undefined },
									t('players.act.banned', { name: player })
								)}
				>
					{asking === 'kick' ? t('players.act.kick') : t('players.act.ban')}
				</Button>
			</Dialog.Footer>
		{:else if asking === 'give'}
			<Dialog.Header>
				<Dialog.Title>{t('players.act.giveTitle', { name: player })}</Dialog.Title>
				<Dialog.Description>{t('players.act.giveHint')}</Dialog.Description>
			</Dialog.Header>
			<div class="grid gap-3">
				<div class="grid gap-2">
					<Label for="item">{t('players.act.item')}</Label>
					<ItemPicker {serverId} bind:value={item} />
				</div>
				<div class="grid w-24 gap-2">
					<Label for="count">{t('players.act.count')}</Label>
					<Input id="count" type="number" min="1" max="6400" bind:value={count} />
				</div>
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}
					>{t('common.actions.cancel')}</Button
				>
				<Button
					disabled={busy || !item.trim()}
					onclick={() =>
						run(
							{ action: 'give', player, item: item.trim(), count: Number(count) || 1 },
							t('players.act.gave', { name: player, item: item.trim() })
						)}
				>
					{t('players.act.give')}
				</Button>
			</Dialog.Footer>
		{:else if asking === 'teleport'}
			<Dialog.Header>
				<Dialog.Title>{t('players.act.teleportTitle', { name: player })}</Dialog.Title>
				<Dialog.Description>{t('players.act.teleportHint')}</Dialog.Description>
			</Dialog.Header>
			<div class="grid gap-3">
				{#if others.length}
					<div class="grid gap-2">
						<Label for="target">{t('players.act.toPlayer')}</Label>
						<div class="flex gap-2">
							<select
								id="target"
								bind:value={target}
								class="h-9 flex-1 rounded-md border border-input bg-transparent px-3 text-sm"
							>
								{#each others as one (one)}<option value={one}>{one}</option>{/each}
							</select>
							<Button
								disabled={busy || !target}
								onclick={() =>
									run(
										{ action: 'teleport', player, to: target },
										t('players.act.teleported', { name: player, to: target })
									)}
							>
								{t('players.act.go')}
							</Button>
						</div>
					</div>
				{/if}
				<div class="grid gap-2">
					<Label for="x">{t('players.act.toPlace')}</Label>
					<div class="flex gap-2">
						<Input id="x" bind:value={x} class="w-20" aria-label="X" />
						<Input bind:value={y} class="w-20" aria-label="Y" />
						<Input bind:value={z} class="w-20" aria-label="Z" />
						<Button
							disabled={busy || !placeOk}
							onclick={() =>
								run(
									{ action: 'teleport', player, to: coordinates },
									t('players.act.teleported', { name: player, to: `${x} ${y} ${z}` })
								)}
						>
							{t('players.act.go')}
						</Button>
					</div>
				</div>
			</div>
		{:else if asking === 'whisper'}
			<Dialog.Header>
				<Dialog.Title>{t('players.act.whisperTitle', { name: player })}</Dialog.Title>
			</Dialog.Header>
			<div class="grid gap-2">
				<Label for="message">{t('players.act.message')}</Label>
				<Input
					id="message"
					bind:value={message}
					onkeydown={(event) =>
						event.key === 'Enter' &&
						message.trim() &&
						run({ action: 'whisper', player, message: message.trim() }, t('players.act.sent'))}
				/>
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}
					>{t('common.actions.cancel')}</Button
				>
				<Button
					disabled={busy || !message.trim()}
					onclick={() =>
						run({ action: 'whisper', player, message: message.trim() }, t('players.act.sent'))}
				>
					{t('players.act.send')}
				</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
