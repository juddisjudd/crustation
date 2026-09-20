<script lang="ts">
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import KeyIcon from '@lucide/svelte/icons/key';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import UsersIcon from '@lucide/svelte/icons/users';
	import { toast } from 'svelte-sonner';
	import PageHeader from '$lib/components/page-header.svelte';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import * as Empty from '$lib/components/ui/empty/index.js';
	import * as Field from '$lib/components/ui/field/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { ApiError } from '$lib/api/client';
	import { errorMessage } from '$lib/api/servers';
	import {
		createUser,
		deleteUser,
		listKeys,
		listRoles,
		listUsers,
		mintKey,
		revokeKey,
		updateUser,
		type ApiKey,
		type Role,
		type UserSummary
	} from '$lib/api/admin';
	import { dateTime } from '$lib/format';
	import { session } from '$lib/session.svelte';
	import { t } from '$lib/i18n/index.svelte';

	let users = $state.raw<UserSummary[]>([]);
	let roles = $state.raw<Role[]>([]);
	let loaded = $state(false);
	let loadFailed = $state(false);

	let editing = $state.raw<UserSummary | null>(null);
	let adding = $state(false);
	let saving = $state(false);
	let fieldErrors = $state.raw<Record<string, string>>({});

	let username = $state('');
	let password = $state('');
	let email = $state('');
	let isAdmin = $state(false);
	let enabled = $state(true);
	let chosenRoles = $state<string[]>([]);

	let keysFor = $state.raw<UserSummary | null>(null);
	let keys = $state.raw<ApiKey[]>([]);
	let keyName = $state('');
	let freshToken = $state('');

	const open = $derived(adding || !!editing);

	async function refresh() {
		loadFailed = false;
		try {
			[users, roles] = await Promise.all([listUsers(), listRoles().catch(() => [])]);
			loaded = true;
		} catch {
			loadFailed = true;
			loaded = true;
		}
	}

	$effect(() => {
		refresh();
	});

	function startAdd() {
		editing = null;
		username = '';
		password = '';
		email = '';
		isAdmin = false;
		enabled = true;
		chosenRoles = [];
		fieldErrors = {};
		adding = true;
	}

	function startEdit(user: UserSummary) {
		adding = false;
		username = user.username;
		password = '';
		email = user.email ?? '';
		isAdmin = user.is_admin;
		enabled = user.enabled;
		chosenRoles = user.roles.map((role) => role.id);
		fieldErrors = {};
		editing = user;
	}

	function close() {
		adding = false;
		editing = null;
	}

	function toggleRole(id: string, on: boolean) {
		chosenRoles = on ? [...chosenRoles, id] : chosenRoles.filter((role) => role !== id);
	}

	async function save() {
		saving = true;
		fieldErrors = {};
		try {
			if (editing) {
				await updateUser(editing.id, {
					username,
					email: email.trim() || null,
					is_admin: isAdmin,
					enabled,
					roles: chosenRoles,
					...(password ? { password } : {})
				});
				toast.success(t('admin.users.done.updated', { name: username }));
			} else {
				await createUser({
					username,
					password,
					email: email.trim() || null,
					is_admin: isAdmin,
					enabled,
					roles: chosenRoles
				});
				toast.success(t('admin.users.done.created', { name: username }));
			}
			close();
			await refresh();
		} catch (err) {
			if (err instanceof ApiError) fieldErrors = err.fields;
			toast.error(t('admin.failed'), { description: errorMessage(err) });
		} finally {
			saving = false;
		}
	}

	async function remove(user: UserSummary) {
		const ok = await confirm({
			title: t('admin.users.confirmDelete', { name: user.username }),
			description: t('admin.users.confirmDeleteBody'),
			confirmLabel: t('common.actions.delete'),
			destructive: true
		});
		if (!ok) return;
		try {
			await deleteUser(user.id);
			toast.success(t('admin.users.done.deleted', { name: user.username }));
			await refresh();
		} catch (err) {
			toast.error(t('admin.failed'), { description: errorMessage(err) });
		}
	}

	async function openKeys(user: UserSummary) {
		keysFor = user;
		keyName = '';
		freshToken = '';
		keys = await listKeys(user.id).catch(() => []);
	}

	async function addKey() {
		if (!keysFor || !keyName.trim()) return;
		try {
			const made = await mintKey(keysFor.id, keyName.trim());
			freshToken = made.token;
			keyName = '';
			keys = await listKeys(keysFor.id);
		} catch (err) {
			toast.error(t('admin.failed'), { description: errorMessage(err) });
		}
	}

	async function dropKey(key: ApiKey) {
		if (!keysFor) return;
		try {
			await revokeKey(keysFor.id, key.id);
			keys = await listKeys(keysFor.id);
		} catch (err) {
			toast.error(t('admin.failed'), { description: errorMessage(err) });
		}
	}
</script>

<svelte:head><title>{t('admin.users.title')} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-5xl px-4 py-8 md:px-8">
	<PageHeader title={t('admin.users.title')} description={t('admin.users.description')}>
		{#snippet actions()}
			<Button onclick={startAdd}>
				<PlusIcon />
				{t('admin.users.new')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if loadFailed}
		<Alert.Root variant="destructive" class="mt-8">
			<Alert.Description>{t('admin.users.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !loaded}
		<Skeleton class="mt-8 h-64 rounded-lg" />
	{:else if users.length === 0}
		<Empty.Root class="mt-8 rounded-lg border border-dashed py-16">
			<Empty.Header>
				<Empty.Media variant="icon"><UsersIcon /></Empty.Media>
				<Empty.Title>{t('admin.users.empty')}</Empty.Title>
			</Empty.Header>
		</Empty.Root>
	{:else}
		<div class="mt-8 overflow-hidden rounded-lg border">
			<Table.Root>
				<Table.Header>
					<Table.Row class="bg-muted/50 hover:bg-muted/50">
						<Table.Head>{t('admin.users.columns.name')}</Table.Head>
						<Table.Head class="hidden md:table-cell">{t('admin.users.columns.roles')}</Table.Head>
						<Table.Head class="hidden lg:table-cell">
							{t('admin.users.columns.lastLogin')}
						</Table.Head>
						<Table.Head class="w-12"></Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each users as user (user.id)}
						<Table.Row>
							<Table.Cell>
								<div class="flex flex-wrap items-center gap-2">
									<span class="font-medium">{user.username}</span>
									{#if user.is_admin}
										<Badge variant="secondary">{t('admin.users.badge.administrator')}</Badge>
									{/if}
									{#if !user.enabled}
										<Badge variant="destructive">{t('admin.users.badge.disabled')}</Badge>
									{/if}
								</div>
							</Table.Cell>
							<Table.Cell class="hidden text-sm text-muted-foreground md:table-cell">
								{user.roles.map((role) => role.name).join(', ') || '—'}
							</Table.Cell>
							<Table.Cell class="hidden text-sm text-muted-foreground lg:table-cell">
								{user.last_login_at ? dateTime(user.last_login_at) : t('admin.users.badge.never')}
							</Table.Cell>
							<Table.Cell>
								<DropdownMenu.Root>
									<DropdownMenu.Trigger>
										{#snippet child({ props })}
											<Button
												{...props}
												variant="ghost"
												size="icon-sm"
												aria-label={t('admin.users.edit', { name: user.username })}
											>
												<EllipsisIcon />
											</Button>
										{/snippet}
									</DropdownMenu.Trigger>
									<DropdownMenu.Content align="end">
										<DropdownMenu.Item onSelect={() => startEdit(user)}>
											<PencilIcon />
											{t('common.actions.edit')}
										</DropdownMenu.Item>
										<DropdownMenu.Item onSelect={() => openKeys(user)}>
											<KeyIcon />
											{t('admin.users.keys.manage')}
										</DropdownMenu.Item>
										{#if user.id !== session.user?.id}
											<DropdownMenu.Separator />
											<DropdownMenu.Item variant="destructive" onSelect={() => remove(user)}>
												<Trash2Icon />
												{t('common.actions.delete')}
											</DropdownMenu.Item>
										{/if}
									</DropdownMenu.Content>
								</DropdownMenu.Root>
							</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
		</div>
	{/if}
</div>

<Dialog.Root {open} onOpenChange={(next) => !next && close()}>
	<Dialog.Content class="max-h-[85vh] overflow-y-auto sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>
				{editing ? t('admin.users.edit', { name: editing.username }) : t('admin.users.new')}
			</Dialog.Title>
		</Dialog.Header>

		<div class="space-y-4">
			<Field.Field data-invalid={!!fieldErrors.username}>
				<Field.Label for="username">{t('admin.users.field.username')}</Field.Label>
				<Input id="username" bind:value={username} autocomplete="off" />
				{#if fieldErrors.username}<Field.Error>{fieldErrors.username}</Field.Error>{/if}
			</Field.Field>

			<Field.Field data-invalid={!!fieldErrors.password}>
				<Field.Label for="password">{t('admin.users.field.password')}</Field.Label>
				<Input id="password" type="password" bind:value={password} autocomplete="new-password" />
				{#if fieldErrors.password}
					<Field.Error>{fieldErrors.password}</Field.Error>
				{:else}
					<Field.Description>
						{editing ? t('admin.users.field.passwordKeep') : t('admin.users.field.passwordHint')}
					</Field.Description>
				{/if}
			</Field.Field>

			<Field.Field>
				<Field.Label for="email">{t('admin.users.field.email')}</Field.Label>
				<Input id="email" type="email" bind:value={email} autocomplete="off" />
			</Field.Field>

			<Field.Field orientation="horizontal">
				<Field.Content>
					<Field.Label for="enabled">{t('admin.users.field.enabled')}</Field.Label>
				</Field.Content>
				<Switch id="enabled" bind:checked={enabled} />
			</Field.Field>

			{#if session.superuser}
				<Field.Field orientation="horizontal">
					<Field.Content>
						<Field.Label for="is-admin">{t('admin.users.field.admin')}</Field.Label>
						<Field.Description>{t('admin.users.field.adminHint')}</Field.Description>
					</Field.Content>
					<Switch id="is-admin" bind:checked={isAdmin} />
				</Field.Field>
			{/if}

			{#if !isAdmin}
				<Field.Field>
					<Field.Label>{t('admin.users.field.roles')}</Field.Label>
					{#if roles.length === 0}
						<Field.Description>{t('admin.users.field.rolesEmpty')}</Field.Description>
					{:else}
						<div class="space-y-2">
							{#each roles as role (role.id)}
								<Label class="font-normal">
									<Checkbox
										checked={chosenRoles.includes(role.id)}
										onCheckedChange={(on) => toggleRole(role.id, on)}
									/>
									{role.name}
								</Label>
							{/each}
						</div>
					{/if}
				</Field.Field>
			{/if}
		</div>

		<Dialog.Footer>
			<Button variant="outline" onclick={close} disabled={saving}>
				{t('common.actions.cancel')}
			</Button>
			<Button onclick={save} disabled={saving || !username.trim()}>
				{#if saving}<Spinner class="size-4" />{/if}
				{saving ? t('common.actions.saving') : t('common.actions.save')}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>

<Dialog.Root
	open={!!keysFor}
	onOpenChange={(next) => {
		if (!next) keysFor = null;
	}}
>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>{t('admin.users.keys.title', { name: keysFor?.username ?? '' })}</Dialog.Title>
			<Dialog.Description>{t('admin.users.keys.description')}</Dialog.Description>
		</Dialog.Header>

		{#if freshToken}
			<Alert.Root>
				<Alert.Description class="space-y-2">
					<p>{t('admin.users.keys.created')}</p>
					<code class="block rounded bg-muted px-2 py-1 font-mono text-xs break-all">
						{freshToken}
					</code>
				</Alert.Description>
			</Alert.Root>
		{/if}

		<div class="flex items-end gap-2">
			<Field.Field>
				<Field.Label for="key-name">{t('admin.users.keys.name')}</Field.Label>
				<Input id="key-name" bind:value={keyName} placeholder="backup script" />
			</Field.Field>
			<Button onclick={addKey} disabled={!keyName.trim()}>{t('admin.users.keys.create')}</Button>
		</div>

		{#if keys.length === 0}
			<p class="text-sm text-muted-foreground">{t('admin.users.keys.empty')}</p>
		{:else}
			<ul class="divide-y rounded-lg border">
				{#each keys as key (key.id)}
					<li class="flex items-center justify-between gap-3 px-3 py-2">
						<div class="min-w-0">
							<p class="truncate text-sm font-medium">{key.name}</p>
							<p class="text-xs text-muted-foreground">
								{key.last_used_at
									? `${t('admin.users.keys.lastUsed')} ${dateTime(key.last_used_at)}`
									: t('admin.users.keys.never')}
							</p>
						</div>
						<Button variant="ghost" size="sm" onclick={() => dropKey(key)}>
							{t('admin.users.keys.revoke')}
						</Button>
					</li>
				{/each}
			</ul>
		{/if}
	</Dialog.Content>
</Dialog.Root>
