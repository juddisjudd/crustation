<script lang="ts">
	import type { PageData } from './$types';
	import KeyRoundIcon from '@lucide/svelte/icons/key-round';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import TrashIcon from '@lucide/svelte/icons/trash-2';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { errorMessage } from '$lib/api/servers';
	import {
		liftBan,
		readBans,
		readRoles,
		readRoster,
		ROLE_PERMISSIONS,
		saveRoles,
		type EnshroudedBan,
		type EnshroudedRole,
		type Roster
	} from '$lib/api/steam';
	import { servers } from '$lib/servers.svelte';
	import { dateTime } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { data }: { data: PageData } = $props();

	const server = $derived(data.server);
	const enshrouded = $derived(server.kind === 'enshrouded');
	const canConfigure = $derived(server.permissions.includes('CONFIG'));
	const live = $derived(servers.isLive(server.id));

	let roster = $state.raw<Roster | null>(null);
	let loadFailed = $state(false);

	$effect(() => {
		const id = server.id;
		let current = true;
		const read = () =>
			readRoster(id)
				.then((found) => {
					if (current) roster = found;
				})
				.catch(() => {
					if (current) loadFailed = true;
				});
		read();
		const timer = setInterval(read, 15_000);
		return () => {
			current = false;
			clearInterval(timer);
		};
	});

	const online = $derived(roster?.known.filter((player) => player.online) ?? []);

	let roles = $state<EnshroudedRole[]>([]);
	let savedRoles = $state('');
	let slots = $state(16);
	let rolesFailed = $state(false);
	let saving = $state(false);
	const rolesChanged = $derived(JSON.stringify(roles) !== savedRoles);

	let bans = $state.raw<EnshroudedBan[] | null>(null);

	async function loadRoles() {
		rolesFailed = false;
		try {
			const found = await readRoles(server.id);
			roles = found.roles;
			savedRoles = JSON.stringify(found.roles);
			slots = found.slots;
		} catch {
			rolesFailed = true;
		}
	}

	async function loadBans() {
		try {
			bans = (await readBans(server.id)).bans;
		} catch {
			bans = [];
		}
	}

	$effect(() => {
		if (!enshrouded) return;
		void server.id;
		void live;
		if (canConfigure) loadRoles();
		loadBans();
	});

	function password() {
		const alphabet = 'abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789';
		const picks = crypto.getRandomValues(new Uint32Array(12));
		return Array.from(picks, (pick) => alphabet[pick % alphabet.length]).join('');
	}

	function addRole() {
		roles.push({
			name: t('players.steam.roles.newName', { count: roles.length + 1 }),
			password: password(),
			canKickBan: false,
			canAccessInventories: false,
			canEditWorld: true,
			canEditBase: false,
			canExtendBase: false,
			reservedSlots: 0
		});
	}

	async function removeRole(index: number) {
		const confirmed = await confirm({
			title: t('players.steam.roles.removeTitle', { name: roles[index].name }),
			description: t('players.steam.roles.removeBody'),
			confirmLabel: t('players.steam.roles.remove'),
			destructive: true
		});
		if (confirmed) roles.splice(index, 1);
	}

	async function save() {
		saving = true;
		try {
			const result = await saveRoles(server.id, roles);
			roles = result.roles;
			savedRoles = JSON.stringify(result.roles);
			toast.success(t('players.steam.roles.saved'));
		} catch (err) {
			toast.error(t('players.steam.roles.saveFailed'), { description: errorMessage(err) });
		} finally {
			saving = false;
		}
	}

	async function lift(ban: EnshroudedBan) {
		const who = ban.name ?? ban.character ?? t('players.steam.bans.someone');
		const confirmed = await confirm({
			title: t('players.steam.bans.liftTitle', { name: who }),
			description: t('players.steam.bans.liftBody'),
			confirmLabel: t('players.steam.bans.lift')
		});
		if (!confirmed) return;
		try {
			await liftBan(server.id, ban.index);
			toast.success(t('players.steam.bans.lifted', { name: who }));
		} catch (err) {
			toast.error(t('players.steam.bans.liftFailed'), { description: errorMessage(err) });
		}
		await loadBans();
	}

	const bannedOn = (seconds: number | null) =>
		seconds === null ? '' : dateTime(new Date(seconds * 1000).toISOString());
	const permissionLabel = (key: (typeof ROLE_PERMISSIONS)[number]) =>
		t(`players.steam.roles.permissions.${key}` as MessageKey);
</script>

<svelte:head><title>{t('players.title')} · {server.name} · Crustation</title></svelte:head>

<div class="page py-4">
	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('players.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !roster}
		<Skeleton class="h-64 rounded-lg" />
	{:else}
		<div class="grid gap-6 xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)] xl:items-start">
			<section class="rounded-lg border bg-card">
				<div class="flex flex-wrap items-center justify-between gap-2 border-b px-5 py-4">
					<h2 class="text-base font-semibold tracking-tight">{t('players.people')}</h2>
					{#if roster.running}
						<span class="text-sm text-muted-foreground tabular-nums">
							{roster.max === null
								? roster.count
								: t('players.online.count', { count: roster.count, max: roster.max })}
						</span>
					{/if}
				</div>

				{#if roster.known.length === 0}
					<p class="px-5 py-8 text-center text-sm text-muted-foreground">
						{t('players.online.none')}
					</p>
				{:else}
					<Table.Root>
						<Table.Header>
							<Table.Row>
								<Table.Head class="pl-5">{t('players.known.name')}</Table.Head>
								<Table.Head>{t('players.known.last')}</Table.Head>
							</Table.Row>
						</Table.Header>
						<Table.Body>
							{#each roster.known as player (player.name)}
								<Table.Row>
									<Table.Cell class="pl-5">
										<span class="font-medium">{player.name}</span>
										{#if player.online}
											<Badge variant="secondary" class="ml-2">{t('players.known.here')}</Badge>
										{/if}
										{#if player.id}
											<span class="block font-mono text-xs text-muted-foreground">{player.id}</span>
										{/if}
									</Table.Cell>
									<Table.Cell class="text-xs whitespace-nowrap text-muted-foreground">
										{dateTime(player.last_seen)}
									</Table.Cell>
								</Table.Row>
							{/each}
						</Table.Body>
					</Table.Root>
				{/if}
				<p class="border-t px-5 py-3 text-xs text-muted-foreground">
					{enshrouded ? t('players.steam.fromConsoleEnshrouded') : t('players.steam.fromConsole')}
					{#if online.length !== roster.count && roster.running}
						{t('players.steam.countDiffers')}
					{/if}
				</p>
			</section>

			<div class="space-y-6">
				{#if enshrouded && canConfigure}
					<section class="rounded-lg border bg-card">
						<div class="flex flex-wrap items-center justify-between gap-2 border-b px-5 py-4">
							<div>
								<h2 class="text-base font-semibold tracking-tight">
									{t('players.steam.roles.title')}
								</h2>
								<p class="mt-0.5 text-sm text-muted-foreground">{t('players.steam.roles.hint')}</p>
							</div>
							<div class="flex items-center gap-2">
								<Button variant="outline" size="sm" disabled={live} onclick={addRole}>
									<PlusIcon />
									{t('players.steam.roles.add')}
								</Button>
								<Button size="sm" disabled={live || !rolesChanged || saving} onclick={save}>
									{#if saving}<Spinner />{/if}
									{t('common.actions.save')}
								</Button>
							</div>
						</div>
						{#if live}
							<p class="border-b bg-muted/40 px-5 py-2 text-sm text-muted-foreground">
								{t('players.steam.stopFirst')}
							</p>
						{/if}
						{#if rolesFailed}
							<p class="px-5 py-6 text-sm text-destructive">
								{t('players.steam.roles.loadFailed')}
							</p>
						{:else}
							<div class="divide-y">
								{#each roles as role, index (index)}
									<fieldset class="grid gap-3 px-5 py-4" disabled={live}>
										<div class="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(0,1.4fr)_auto]">
											<div class="grid gap-1.5">
												<Label for="role-{index}-name">{t('players.steam.roles.name')}</Label>
												<Input id="role-{index}-name" bind:value={role.name} />
											</div>
											<div class="grid gap-1.5">
												<Label for="role-{index}-password"
													>{t('players.steam.roles.password')}</Label
												>
												<div class="flex gap-1.5">
													<Input
														id="role-{index}-password"
														bind:value={role.password}
														class="font-mono"
														autocomplete="off"
														spellcheck={false}
													/>
													<Button
														variant="outline"
														size="icon"
														aria-label={t('players.steam.roles.newPassword')}
														title={t('players.steam.roles.newPassword')}
														onclick={() => (role.password = password())}
													>
														<KeyRoundIcon />
													</Button>
												</div>
											</div>
											<div class="grid gap-1.5">
												<Label for="role-{index}-slots">{t('players.steam.roles.reserved')}</Label>
												<Input
													id="role-{index}-slots"
													type="number"
													min="0"
													max={slots}
													bind:value={role.reservedSlots}
													class="w-24 tabular-nums"
												/>
											</div>
										</div>
										<div class="flex flex-wrap items-center gap-x-5 gap-y-2">
											{#each ROLE_PERMISSIONS as key (key)}
												<Label class="font-normal">
													<Checkbox bind:checked={role[key]} />
													{permissionLabel(key)}
												</Label>
											{/each}
											<Button
												variant="ghost"
												size="sm"
												class="ml-auto text-muted-foreground"
												onclick={() => removeRole(index)}
											>
												<TrashIcon />
												{t('players.steam.roles.remove')}
											</Button>
										</div>
									</fieldset>
								{:else}
									<p class="px-5 py-6 text-sm text-muted-foreground">
										{t('players.steam.roles.none')}
									</p>
								{/each}
							</div>
						{/if}
					</section>
				{/if}

				{#if enshrouded}
					<section class="rounded-lg border bg-card">
						<div class="border-b px-5 py-4">
							<h2 class="text-base font-semibold tracking-tight">
								{t('players.steam.bans.title')}
							</h2>
							<p class="mt-0.5 text-sm text-muted-foreground">{t('players.steam.bans.hint')}</p>
						</div>
						{#if bans === null}
							<Skeleton class="m-5 h-10" />
						{:else if bans.length === 0}
							<p class="px-5 py-6 text-sm text-muted-foreground">{t('players.steam.bans.none')}</p>
						{:else}
							<Table.Root>
								<Table.Body>
									{#each bans as ban (ban.index)}
										<Table.Row>
											<Table.Cell class="pl-5">
												<span class="font-medium">
													{ban.name ?? t('players.steam.bans.someone')}
												</span>
												{#if ban.character}
													<span class="block text-xs text-muted-foreground">{ban.character}</span>
												{/if}
											</Table.Cell>
											<Table.Cell class="text-xs whitespace-nowrap text-muted-foreground">
												{bannedOn(ban.banned_at)}
											</Table.Cell>
											<Table.Cell class="w-10 pr-5">
												<Button
													variant="outline"
													size="xs"
													disabled={live}
													title={live ? t('players.steam.stopFirst') : undefined}
													onclick={() => lift(ban)}
												>
													<UndoIcon />
													{t('players.steam.bans.lift')}
												</Button>
											</Table.Cell>
										</Table.Row>
									{/each}
								</Table.Body>
							</Table.Root>
						{/if}
					</section>
				{:else}
					<section class="rounded-lg border bg-card px-5 py-4">
						<h2 class="text-base font-semibold tracking-tight">
							{t('players.steam.dragonwilds.title')}
						</h2>
						<p class="mt-2 text-sm text-muted-foreground">{t('players.steam.dragonwilds.owner')}</p>
						<p class="mt-2 text-sm text-muted-foreground">
							{t('players.steam.dragonwilds.admins')}
						</p>
					</section>
				{/if}
			</div>
		</div>
	{/if}
</div>
