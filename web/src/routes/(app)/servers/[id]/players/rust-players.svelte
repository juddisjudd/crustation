<script lang="ts">
	import type { PageData } from './$types';
	import BanIcon from '@lucide/svelte/icons/ban';
	import BootIcon from '@lucide/svelte/icons/door-open';
	import ExternalLinkIcon from '@lucide/svelte/icons/external-link';
	import GiftIcon from '@lucide/svelte/icons/gift';
	import HandHeartIcon from '@lucide/svelte/icons/hand-heart';
	import HeartPulseIcon from '@lucide/svelte/icons/heart-pulse';
	import MapPinIcon from '@lucide/svelte/icons/map-pin';
	import MicOffIcon from '@lucide/svelte/icons/mic-off';
	import MicIcon from '@lucide/svelte/icons/mic';
	import MoreIcon from '@lucide/svelte/icons/ellipsis';
	import PackageXIcon from '@lucide/svelte/icons/package-x';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SendIcon from '@lucide/svelte/icons/send';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import SkullIcon from '@lucide/svelte/icons/skull';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { errorMessage } from '$lib/api/servers';
	import {
		actOnRustPlayer,
		isSteamId,
		rustPlayers,
		steamProfileUrl,
		type RustPlayerAction,
		type RustPlayers,
		type RustRole
	} from '$lib/api/rust';
	import { dateTime } from '$lib/format';
	import ItemPicker from '$lib/components/item-picker.svelte';
	import RustPlayerSheet from './rust-player-sheet.svelte';
	import RustReports from './rust-reports.svelte';
	import RustSharedBans from './rust-shared-bans.svelte';
	import type { Person, Verb } from './rust-person';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { data }: { data: PageData } = $props();

	const server = $derived(data.server);
	const canCommand = $derived(server.permissions.includes('COMMANDS'));

	const LENGTHS = [
		['forever', ''],
		['h1', '1h'],
		['h12', '12h'],
		['d1', '1d'],
		['d7', '7d'],
		['d30', '30d']
	] as const;
	const lengthLabel = (key: string) => t(`players.rust.lengths.${key}` as MessageKey);

	let overview = $state.raw<RustPlayers | null>(null);
	let loadFailed = $state(false);
	let busy = $state(false);
	let list = $state<'bans' | 'staff' | 'shared' | 'reports'>('bans');
	let broadcast = $state('');

	let newId = $state('');
	let newName = $state('');
	let newReason = $state('');
	let newLength = $state('');
	let newRole = $state<RustRole>('moderator');

	type Asking = 'kick' | 'ban' | 'mute' | 'give' | 'teleport';
	let asking = $state<Asking | null>(null);
	let subject = $state<Person | null>(null);
	let reason = $state('');
	let length = $state('');
	let item = $state('');
	let amount = $state(1);
	let target = $state('');

	let viewing = $state<string | null>(null);
	let viewOpen = $state(false);

	/** One row per person: on now, seen before, or only named on a list. */
	const people = $derived.by(() => {
		if (!overview) return [];
		const rows = new Map<string, Person>();
		const key = (steamId: string | null, name: string) => steamId ?? `name:${name.toLowerCase()}`;
		for (const seen of overview.known) {
			rows.set(key(seen.steam_id, seen.name), {
				steamId: seen.steam_id,
				name: seen.name,
				on: false,
				first: seen.first_seen,
				last: seen.last_seen,
				banned: false
			});
		}
		for (const ban of overview.bans) {
			const found = rows.get(ban.steam_id);
			rows.set(ban.steam_id, {
				...(found ?? { steamId: ban.steam_id, name: ban.name || ban.steam_id, on: false }),
				banned: true,
				ban
			});
		}
		for (const member of overview.staff) {
			const found = rows.get(member.steam_id);
			rows.set(member.steam_id, {
				...(found ?? {
					steamId: member.steam_id,
					name: member.name || member.steam_id,
					on: false,
					banned: false
				}),
				role: member.role
			});
		}
		for (const here of overview.online) {
			const found = rows.get(here.steam_id);
			rows.set(here.steam_id, {
				...(found ?? { steamId: here.steam_id, banned: false }),
				name: here.name,
				on: true,
				ping: here.ping,
				health: here.health,
				address: here.address,
				connectedSeconds: here.connected_seconds
			});
		}
		return [...rows.values()].sort((a, b) => {
			if (a.on !== b.on) return a.on ? -1 : 1;
			if (a.last !== b.last) return (b.last ?? '').localeCompare(a.last ?? '');
			return a.name.localeCompare(b.name);
		});
	});

	const onNow = $derived(people.filter((one) => one.on && one.steamId));
	const viewed = $derived(people.find((one) => one.steamId === viewing) ?? null);

	async function load() {
		loadFailed = false;
		try {
			overview = await rustPlayers(server.id);
		} catch {
			loadFailed = true;
		}
	}

	$effect(() => {
		void server.id;
		load();
		// Websocket RCON leaves the console alone, so asking every few seconds costs nothing visible.
		const timer = setInterval(() => overview?.running && load(), 10_000);
		return () => clearInterval(timer);
	});

	async function run(action: RustPlayerAction) {
		busy = true;
		try {
			const acted = await actOnRustPlayer(server.id, action);
			toast.success(acted.output || t('players.act.sent'));
			asking = null;
			await load();
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function sure(action: 'kill' | 'clear-inventory', person: Person) {
		if (!person.steamId) return;
		const kill = action === 'kill';
		const agreed = await confirm({
			title: t(kill ? 'players.rust.killTitle' : 'players.rust.clearTitle', { name: person.name }),
			description: t(kill ? 'players.rust.killBody' : 'players.rust.clearBody'),
			confirmLabel: t(kill ? 'players.rust.kill' : 'players.rust.clearInventory'),
			destructive: true
		});
		if (agreed) await run({ action, steam_id: person.steamId });
	}

	async function setRole(person: Person, next: string) {
		if (!person.steamId) return;
		const role = next === 'owner' || next === 'moderator' ? next : null;
		await run({ action: 'staff', steam_id: person.steamId, name: person.name, role });
	}

	function act(verb: Verb, person: Person) {
		if (!person.steamId) return;
		switch (verb) {
			case 'kick':
			case 'ban':
			case 'mute':
			case 'give':
			case 'teleport':
				return open(verb, person);
			case 'kill':
			case 'clear-inventory':
				return sure(verb, person);
			default:
				return run({ action: verb, steam_id: person.steamId });
		}
	}

	function open(kind: Asking, person: Person) {
		subject = person;
		reason = '';
		length = '';
		item = '';
		amount = 1;
		target = '';
		asking = kind;
	}

	async function add() {
		const id = newId.trim();
		if (!isSteamId(id)) {
			toast.error(t('players.rust.badId'));
			return;
		}
		const name = newName.trim() || undefined;
		await run(
			list === 'bans'
				? {
						action: 'ban',
						steam_id: id,
						name,
						reason: newReason.trim() || undefined,
						duration: newLength || undefined
					}
				: { action: 'staff', steam_id: id, name, role: newRole }
		);
		newId = '';
		newName = '';
		newReason = '';
	}

	async function say() {
		const message = broadcast.trim();
		if (!message) return;
		await run({ action: 'say', message });
		broadcast = '';
	}

	const expiry = (expires: number | null) =>
		expires === null
			? t('players.rust.lengths.forever')
			: t('players.rust.until', { when: dateTime(new Date(expires * 1000).toISOString()) });
	const roleLabel = (role: RustRole) =>
		role === 'owner' ? t('players.rust.owner') : t('players.rust.moderator');
</script>

<svelte:head><title>{t('players.title')} · {server.name} · Crustation</title></svelte:head>

<div class="page py-4">
	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('players.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !overview}
		<Skeleton class="h-64 rounded-lg" />
	{:else}
		{@const state = overview}
		<div class="grid gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] xl:items-start">
			<section class="rounded-lg border bg-card">
				<div class="flex flex-wrap items-center justify-between gap-2 border-b px-5 py-4">
					<h2 class="text-base font-semibold tracking-tight">{t('players.people')}</h2>
					{#if state.running}
						<div class="flex items-center gap-3">
							{#if canCommand}
								<Button
									variant="outline"
									size="xs"
									disabled={busy || state.online.length === 0}
									onclick={() => run({ action: 'heal-all' })}
								>
									<HeartPulseIcon />
									{t('players.rust.healAll')}
								</Button>
							{/if}
							<span class="text-sm text-muted-foreground tabular-nums">{state.online.length}</span>
						</div>
					{/if}
				</div>

				{#if people.length === 0}
					<p class="px-5 py-8 text-center text-sm text-muted-foreground">
						{t('players.online.none')}
					</p>
				{:else}
					<Table.Root>
						<Table.Body>
							{#each people as person (person.steamId ?? person.name)}
								<Table.Row>
									<Table.Cell class="py-2">
										<div class="flex flex-wrap items-center gap-2">
											{#if person.steamId}
												<button
													type="button"
													class="cursor-pointer font-medium underline-offset-4 hover:underline"
													onclick={() => {
														viewing = person.steamId;
														viewOpen = true;
													}}
												>
													{person.name}
												</button>
											{:else}
												<span class="font-medium">{person.name}</span>
											{/if}
											{#if person.role}
												<Badge variant="secondary" class="gap-1">
													<ShieldIcon class="size-3" />
													{roleLabel(person.role)}
												</Badge>
											{/if}
											{#if person.banned}
												<Badge variant="destructive" class="gap-1">
													<BanIcon class="size-3" />
													{t('players.badge.banned')}
												</Badge>
											{/if}
										</div>
										{#if person.steamId}
											<span class="font-mono text-xs text-muted-foreground">{person.steamId}</span>
										{/if}
									</Table.Cell>
									<Table.Cell
										class="py-2 text-right text-xs whitespace-nowrap text-muted-foreground"
									>
										{#if person.on}
											<span class="text-success">{t('players.known.here')}</span>
											{#if person.ping !== undefined}
												<span class="ml-1 tabular-nums">
													{t('players.rust.ping', { ping: person.ping })}
												</span>
											{/if}
										{:else if person.last}
											{dateTime(person.last)}
										{/if}
									</Table.Cell>
									<Table.Cell class="w-10 py-2">
										{#if person.steamId}
											{@render actions(person, state.running)}
										{/if}
									</Table.Cell>
								</Table.Row>
							{/each}
						</Table.Body>
					</Table.Root>
				{/if}

				{#if canCommand}
					<form
						class="flex gap-2 border-t px-5 py-3"
						onsubmit={(event) => {
							event.preventDefault();
							say();
						}}
					>
						<Input
							bind:value={broadcast}
							placeholder={t('players.act.sayPlaceholder')}
							aria-label={t('players.act.say')}
							disabled={!state.running}
						/>
						<Button type="submit" disabled={busy || !state.running || !broadcast.trim()}>
							<SendIcon />
							{t('players.act.say')}
						</Button>
					</form>
				{/if}
			</section>

			<section class="rounded-lg border bg-card">
				<div class="border-b px-5 py-4">
					<h2 class="text-base font-semibold tracking-tight">{t('players.listsTitle')}</h2>
				</div>

				<div class="px-5 py-4">
					<Tabs.Root bind:value={list}>
						<Tabs.List>
							<Tabs.Trigger value="bans">{t('players.rust.bans')}</Tabs.Trigger>
							<Tabs.Trigger value="staff">{t('players.rust.staff')}</Tabs.Trigger>
							<Tabs.Trigger value="shared">{t('players.rust.shared')}</Tabs.Trigger>
							<Tabs.Trigger value="reports">{t('players.rust.reports')}</Tabs.Trigger>
						</Tabs.List>
					</Tabs.Root>

					{#if list === 'shared'}
						<RustSharedBans {server} />
					{:else if list === 'reports'}
						<RustReports {server} />
					{:else}
						<form
							class="mt-4 flex flex-wrap items-end gap-2"
							onsubmit={(event) => {
								event.preventDefault();
								add();
							}}
						>
							<Input
								bind:value={newId}
								placeholder={t('players.rust.steamId')}
								aria-label={t('players.rust.steamId')}
								class="w-48 font-mono"
								inputmode="numeric"
							/>
							<Input
								bind:value={newName}
								placeholder={t('players.add.name')}
								aria-label={t('players.add.name')}
								class="w-36"
							/>
							{#if list === 'bans'}
								<Input
									bind:value={newReason}
									placeholder={t('players.add.reason')}
									aria-label={t('players.add.reason')}
									class="w-40"
								/>
								{@render lengthPicker(newLength, (next) => (newLength = next), 'w-32')}
							{:else}
								<Select.Root type="single" bind:value={newRole}>
									<Select.Trigger class="w-36" aria-label={t('players.rust.role')}>
										{roleLabel(newRole)}
									</Select.Trigger>
									<Select.Content>
										<Select.Item value="moderator" label={t('players.rust.moderator')} />
										<Select.Item value="owner" label={t('players.rust.owner')} />
									</Select.Content>
								</Select.Root>
							{/if}
							<Button type="submit" disabled={busy || !newId.trim()}>
								<PlusIcon />
								{t('players.add.button')}
							</Button>
						</form>

						{#if list === 'bans' ? state.bans.length : state.staff.length}
							<div class="mt-4 overflow-x-auto rounded-lg border">
								<Table.Root>
									<Table.Body>
										{#if list === 'bans'}
											{#each state.bans as ban (ban.steam_id)}
												<Table.Row>
													<Table.Cell>
														<span class="font-medium">{ban.name || ban.steam_id}</span>
														<span class="block font-mono text-xs text-muted-foreground">
															{ban.steam_id}
														</span>
													</Table.Cell>
													<Table.Cell class="max-w-56 truncate text-xs text-muted-foreground">
														{ban.reason}
													</Table.Cell>
													<Table.Cell class="text-xs whitespace-nowrap text-muted-foreground">
														{expiry(ban.expires)}
													</Table.Cell>
													<Table.Cell class="w-10">
														<Button
															variant="ghost"
															size="icon-sm"
															aria-label={t('players.remove', { name: ban.name || ban.steam_id })}
															disabled={busy}
															onclick={() => run({ action: 'unban', steam_id: ban.steam_id })}
														>
															<XIcon />
														</Button>
													</Table.Cell>
												</Table.Row>
											{/each}
										{:else}
											{#each state.staff as member (member.steam_id)}
												<Table.Row>
													<Table.Cell>
														<span class="font-medium">{member.name || member.steam_id}</span>
														<span class="block font-mono text-xs text-muted-foreground">
															{member.steam_id}
														</span>
													</Table.Cell>
													<Table.Cell class="text-xs text-muted-foreground">
														{roleLabel(member.role)}
													</Table.Cell>
													<Table.Cell class="w-10">
														<Button
															variant="ghost"
															size="icon-sm"
															aria-label={t('players.remove', {
																name: member.name || member.steam_id
															})}
															disabled={busy}
															onclick={() =>
																run({ action: 'staff', steam_id: member.steam_id, role: null })}
														>
															<XIcon />
														</Button>
													</Table.Cell>
												</Table.Row>
											{/each}
										{/if}
									</Table.Body>
								</Table.Root>
							</div>
						{:else}
							<p class="mt-4 text-sm text-muted-foreground">{t('players.empty')}</p>
						{/if}

						<p class="mt-3 text-xs text-muted-foreground">
							{t('players.rust.fileHint', { file: list === 'bans' ? 'bans.cfg' : 'users.cfg' })}
						</p>
					{/if}
				</div>
			</section>
		</div>
	{/if}
</div>

{#snippet lengthPicker(value: string, choose: (next: string) => void, width: string)}
	<Select.Root type="single" {value} onValueChange={choose}>
		<Select.Trigger class={width} aria-label={t('players.rust.length')}>
			{lengthLabel(LENGTHS.find(([, code]) => code === value)?.[0] ?? 'forever')}
		</Select.Trigger>
		<Select.Content>
			{#each LENGTHS as [key, code] (key)}
				<Select.Item value={code} label={lengthLabel(key)} />
			{/each}
		</Select.Content>
	</Select.Root>
{/snippet}

{#snippet actions(person: Person, running: boolean)}
	{@const steamId = person.steamId ?? ''}
	{@const here = running && person.on}
	<DropdownMenu.Root>
		<DropdownMenu.Trigger>
			{#snippet child({ props })}
				<Button
					{...props}
					variant="ghost"
					size="icon-sm"
					aria-label={t('players.act.menu', { name: person.name })}
				>
					<MoreIcon />
				</Button>
			{/snippet}
		</DropdownMenu.Trigger>
		<DropdownMenu.Content align="end" class="w-52">
			{#if here}
				<DropdownMenu.Item onSelect={() => open('kick', person)}>
					<BootIcon />
					{t('players.act.kick')}
				</DropdownMenu.Item>
			{/if}
			{#if person.banned}
				<DropdownMenu.Item onSelect={() => run({ action: 'unban', steam_id: steamId })}>
					<UndoIcon />
					{t('players.act.pardon')}
				</DropdownMenu.Item>
			{:else}
				<DropdownMenu.Item variant="destructive" onSelect={() => open('ban', person)}>
					<BanIcon />
					{t('players.act.ban')}
				</DropdownMenu.Item>
			{/if}
			{#if here}
				<DropdownMenu.Item onSelect={() => open('mute', person)}>
					<MicOffIcon />
					{t('players.rust.mute')}
				</DropdownMenu.Item>
				<DropdownMenu.Item onSelect={() => run({ action: 'unmute', steam_id: steamId })}>
					<MicIcon />
					{t('players.rust.unmute')}
				</DropdownMenu.Item>
			{/if}
			<DropdownMenu.Separator />
			<DropdownMenu.Sub>
				<DropdownMenu.SubTrigger>
					<ShieldIcon />
					{t('players.rust.staffRole')}
				</DropdownMenu.SubTrigger>
				<DropdownMenu.SubContent>
					<DropdownMenu.RadioGroup
						value={person.role ?? 'none'}
						onValueChange={(next) => setRole(person, next)}
					>
						<DropdownMenu.RadioItem value="none">{t('players.rust.noRole')}</DropdownMenu.RadioItem>
						<DropdownMenu.RadioItem value="moderator">
							{t('players.rust.moderator')}
						</DropdownMenu.RadioItem>
						<DropdownMenu.RadioItem value="owner">{t('players.rust.owner')}</DropdownMenu.RadioItem>
					</DropdownMenu.RadioGroup>
				</DropdownMenu.SubContent>
			</DropdownMenu.Sub>
			{#if canCommand && here}
				<DropdownMenu.Separator />
				<DropdownMenu.Item onSelect={() => open('give', person)}>
					<GiftIcon />
					{t('players.act.give')}
				</DropdownMenu.Item>
				<DropdownMenu.Item disabled={onNow.length < 2} onSelect={() => open('teleport', person)}>
					<MapPinIcon />
					{t('players.act.teleport')}
				</DropdownMenu.Item>
				<DropdownMenu.Separator />
				<DropdownMenu.Item onSelect={() => run({ action: 'heal', steam_id: steamId })}>
					<HeartPulseIcon />
					{t('players.rust.heal')}
				</DropdownMenu.Item>
				<DropdownMenu.Item onSelect={() => run({ action: 'revive', steam_id: steamId })}>
					<HandHeartIcon />
					{t('players.rust.revive')}
				</DropdownMenu.Item>
				<DropdownMenu.Item variant="destructive" onSelect={() => sure('kill', person)}>
					<SkullIcon />
					{t('players.rust.kill')}
				</DropdownMenu.Item>
				<DropdownMenu.Item variant="destructive" onSelect={() => sure('clear-inventory', person)}>
					<PackageXIcon />
					{t('players.rust.clearInventory')}
				</DropdownMenu.Item>
			{/if}
			<DropdownMenu.Separator />
			<DropdownMenu.Item>
				{#snippet child({ props })}
					<a {...props} href={steamProfileUrl(steamId)} target="_blank" rel="noreferrer">
						<ExternalLinkIcon />
						{t('players.rust.profile')}
					</a>
				{/snippet}
			</DropdownMenu.Item>
		</DropdownMenu.Content>
	</DropdownMenu.Root>
{/snippet}

<RustPlayerSheet
	person={viewed}
	bind:open={viewOpen}
	running={overview?.running ?? false}
	{canCommand}
	canTeleport={onNow.length > 1}
	{busy}
	{act}
	{setRole}
/>

<Dialog.Root open={asking !== null} onOpenChange={(next) => !next && (asking = null)}>
	<Dialog.Content class="sm:max-w-md">
		{@const who = subject?.steamId ?? ''}
		{@const name = subject?.name ?? ''}
		{#if asking === 'kick' || asking === 'ban'}
			<Dialog.Header>
				<Dialog.Title>
					{asking === 'kick'
						? t('players.act.kickTitle', { name })
						: t('players.act.banTitle', { name })}
				</Dialog.Title>
				<Dialog.Description>{t('players.act.reasonHint')}</Dialog.Description>
			</Dialog.Header>
			<div class="grid gap-3">
				<div class="grid gap-2">
					<Label for="reason">{t('players.add.reason')}</Label>
					<Input id="reason" bind:value={reason} placeholder={t('players.act.reasonPlaceholder')} />
				</div>
				{#if asking === 'ban'}
					<div class="grid gap-2">
						<Label>{t('players.rust.length')}</Label>
						{@render lengthPicker(length, (next) => (length = next), 'w-40')}
					</div>
				{/if}
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}>
					{t('common.actions.cancel')}
				</Button>
				<Button
					variant="destructive"
					disabled={busy}
					onclick={() =>
						asking === 'kick'
							? run({ action: 'kick', steam_id: who, reason: reason.trim() || undefined })
							: run({
									action: 'ban',
									steam_id: who,
									name,
									reason: reason.trim() || undefined,
									duration: length || undefined
								})}
				>
					{asking === 'kick' ? t('players.act.kick') : t('players.act.ban')}
				</Button>
			</Dialog.Footer>
		{:else if asking === 'mute'}
			<Dialog.Header>
				<Dialog.Title>{t('players.rust.mute')} · {name}</Dialog.Title>
			</Dialog.Header>
			<div class="grid gap-2">
				<Label>{t('players.rust.length')}</Label>
				{@render lengthPicker(length, (next) => (length = next), 'w-40')}
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}>
					{t('common.actions.cancel')}
				</Button>
				<Button
					disabled={busy}
					onclick={() => run({ action: 'mute', steam_id: who, duration: length || undefined })}
				>
					{t('players.rust.mute')}
				</Button>
			</Dialog.Footer>
		{:else if asking === 'give'}
			<Dialog.Header>
				<Dialog.Title>{t('players.act.giveTitle', { name })}</Dialog.Title>
			</Dialog.Header>
			<div class="grid gap-4">
				<ItemPicker serverId={server.id} bind:value={item} />
				<div class="grid w-28 gap-2">
					<Label for="amount">{t('players.act.count')}</Label>
					<Input id="amount" type="number" min="1" bind:value={amount} />
				</div>
			</div>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}>
					{t('common.actions.cancel')}
				</Button>
				<Button
					disabled={busy || !item.trim()}
					onclick={() =>
						run({
							action: 'give',
							steam_id: who,
							item: item.trim(),
							amount: Number(amount) || 1
						})}
				>
					{t('players.act.give')}
				</Button>
			</Dialog.Footer>
		{:else if asking === 'teleport'}
			<Dialog.Header>
				<Dialog.Title>{t('players.act.teleportTitle', { name })}</Dialog.Title>
			</Dialog.Header>
			<Select.Root type="single" bind:value={target}>
				<Select.Trigger class="w-full">
					{onNow.find((one) => one.steamId === target)?.name ?? t('players.rust.pickPlayer')}
				</Select.Trigger>
				<Select.Content>
					{#each onNow.filter((one) => one.steamId !== who) as other (other.steamId)}
						<Select.Item value={other.steamId ?? ''} label={other.name} />
					{/each}
				</Select.Content>
			</Select.Root>
			<Dialog.Footer>
				<Button variant="outline" onclick={() => (asking = null)}>
					{t('common.actions.cancel')}
				</Button>
				<Button
					disabled={busy || !target}
					onclick={() => run({ action: 'teleport', steam_id: who, to: target })}
				>
					{t('players.act.go')}
				</Button>
			</Dialog.Footer>
		{/if}
	</Dialog.Content>
</Dialog.Root>
