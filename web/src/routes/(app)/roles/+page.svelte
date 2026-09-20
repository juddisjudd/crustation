<script lang="ts">
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import { toast } from 'svelte-sonner';
	import PageHeader from '$lib/components/page-header.svelte';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
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
		GLOBAL_PERMISSIONS,
		SERVER_PERMISSIONS,
		createRole,
		deleteRole,
		listRoles,
		updateRole,
		type Role
	} from '$lib/api/admin';
	import type { GlobalPermission, ServerPermission } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { t, plural } from '$lib/i18n/index.svelte';

	let roles = $state.raw<Role[]>([]);
	let loaded = $state(false);
	let loadFailed = $state(false);

	let editing = $state.raw<Role | null>(null);
	let adding = $state(false);
	let saving = $state(false);
	let fieldErrors = $state.raw<Record<string, string>>({});

	let name = $state('');
	let global = $state<GlobalPermission[]>([]);
	/** server id to the permissions ticked for it. */
	let grants = $state<Record<string, ServerPermission[]>>({});

	const open = $derived(adding || !!editing);
	const offered = $derived(
		GLOBAL_PERMISSIONS.filter((permission) => permission !== 'ADMIN' || session.superuser)
	);

	async function refresh() {
		loadFailed = false;
		try {
			roles = await listRoles();
			loaded = true;
		} catch {
			loadFailed = true;
			loaded = true;
		}
	}

	$effect(() => {
		refresh();
		servers.refresh().catch(() => {});
	});

	function startAdd() {
		editing = null;
		name = '';
		global = [];
		grants = {};
		fieldErrors = {};
		adding = true;
	}

	function startEdit(role: Role) {
		adding = false;
		name = role.name;
		global = [...role.global_permissions];
		grants = Object.fromEntries(
			role.servers.map((grant) => [grant.server_id, [...grant.permissions]])
		);
		fieldErrors = {};
		editing = role;
	}

	function close() {
		adding = false;
		editing = null;
	}

	function toggleGlobal(permission: GlobalPermission, on: boolean) {
		global = on ? [...global, permission] : global.filter((held) => held !== permission);
	}

	function toggleServer(serverId: string, permission: ServerPermission, on: boolean) {
		const held = grants[serverId] ?? [];
		grants[serverId] = on ? [...held, permission] : held.filter((one) => one !== permission);
	}

	async function save() {
		saving = true;
		fieldErrors = {};
		const body = {
			name,
			global_permissions: global,
			servers: Object.entries(grants)
				.filter(([, permissions]) => permissions.length > 0)
				.map(([server_id, permissions]) => ({ server_id, permissions }))
		};
		try {
			if (editing) {
				await updateRole(editing.id, body);
				toast.success(t('admin.roles.done.updated', { name }));
			} else {
				await createRole(body);
				toast.success(t('admin.roles.done.created', { name }));
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

	async function remove(role: Role) {
		const ok = await confirm({
			title: t('admin.roles.confirmDelete', { name: role.name }),
			description: t('admin.roles.confirmDeleteBody'),
			confirmLabel: t('common.actions.delete'),
			destructive: true
		});
		if (!ok) return;
		try {
			await deleteRole(role.id);
			toast.success(t('admin.roles.done.deleted', { name: role.name }));
			await refresh();
		} catch (err) {
			toast.error(t('admin.failed'), { description: errorMessage(err) });
		}
	}
</script>

<svelte:head><title>{t('admin.roles.title')} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-5xl px-4 py-8 md:px-8">
	<PageHeader title={t('admin.roles.title')} description={t('admin.roles.description')}>
		{#snippet actions()}
			<Button onclick={startAdd}>
				<PlusIcon />
				{t('admin.roles.new')}
			</Button>
		{/snippet}
	</PageHeader>

	{#if loadFailed}
		<Alert.Root variant="destructive" class="mt-8">
			<Alert.Description>{t('admin.roles.loadFailed')}</Alert.Description>
		</Alert.Root>
	{:else if !loaded}
		<Skeleton class="mt-8 h-64 rounded-lg" />
	{:else if roles.length === 0}
		<Empty.Root class="mt-8 rounded-lg border border-dashed py-16">
			<Empty.Header>
				<Empty.Media variant="icon"><ShieldIcon /></Empty.Media>
				<Empty.Title>{t('admin.roles.empty')}</Empty.Title>
			</Empty.Header>
		</Empty.Root>
	{:else}
		<div class="mt-8 overflow-hidden rounded-lg border">
			<Table.Root>
				<Table.Header>
					<Table.Row class="bg-muted/50 hover:bg-muted/50">
						<Table.Head>{t('admin.roles.columns.name')}</Table.Head>
						<Table.Head class="hidden md:table-cell">
							{t('admin.roles.columns.permissions')}
						</Table.Head>
						<Table.Head class="hidden lg:table-cell">
							{t('admin.roles.columns.servers')}
						</Table.Head>
						<Table.Head class="w-28">{t('admin.roles.columns.members')}</Table.Head>
						<Table.Head class="w-12"></Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each roles as role (role.id)}
						<Table.Row>
							<Table.Cell class="font-medium">{role.name}</Table.Cell>
							<Table.Cell class="hidden md:table-cell">
								{#if role.global_permissions.length === 0}
									<span class="text-sm text-muted-foreground">{t('admin.roles.counts.none')}</span>
								{:else}
									<div class="flex flex-wrap gap-1">
										{#each role.global_permissions as permission (permission)}
											<Badge variant="secondary">{t(`common.permission.${permission}`)}</Badge>
										{/each}
									</div>
								{/if}
							</Table.Cell>
							<Table.Cell class="hidden text-sm text-muted-foreground lg:table-cell">
								{role.servers.length
									? plural('admin.roles.counts.servers', role.servers.length)
									: t('admin.roles.counts.none')}
							</Table.Cell>
							<Table.Cell class="text-sm text-muted-foreground">
								{plural('admin.roles.counts.members', role.members)}
							</Table.Cell>
							<Table.Cell>
								<DropdownMenu.Root>
									<DropdownMenu.Trigger>
										{#snippet child({ props })}
											<Button
												{...props}
												variant="ghost"
												size="icon-sm"
												aria-label={t('admin.roles.edit', { name: role.name })}
											>
												<EllipsisIcon />
											</Button>
										{/snippet}
									</DropdownMenu.Trigger>
									<DropdownMenu.Content align="end">
										<DropdownMenu.Item onSelect={() => startEdit(role)}>
											<PencilIcon />
											{t('common.actions.edit')}
										</DropdownMenu.Item>
										<DropdownMenu.Separator />
										<DropdownMenu.Item variant="destructive" onSelect={() => remove(role)}>
											<Trash2Icon />
											{t('common.actions.delete')}
										</DropdownMenu.Item>
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
	<Dialog.Content class="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
		<Dialog.Header>
			<Dialog.Title>
				{editing ? t('admin.roles.edit', { name: editing.name }) : t('admin.roles.new')}
			</Dialog.Title>
		</Dialog.Header>

		<div class="space-y-6">
			<Field.Field data-invalid={!!fieldErrors.name}>
				<Field.Label for="role-name">{t('admin.roles.field.name')}</Field.Label>
				<Input id="role-name" bind:value={name} autocomplete="off" />
				{#if fieldErrors.name}<Field.Error>{fieldErrors.name}</Field.Error>{/if}
			</Field.Field>

			<Field.Field data-invalid={!!fieldErrors.global_permissions}>
				<Field.Label>{t('admin.roles.field.global')}</Field.Label>
				<div class="grid gap-2 sm:grid-cols-2">
					{#each offered as permission (permission)}
						<Label class="font-normal">
							<Checkbox
								checked={global.includes(permission)}
								onCheckedChange={(on) => toggleGlobal(permission, on)}
							/>
							{t(`common.permission.${permission}`)}
						</Label>
					{/each}
				</div>
				{#if fieldErrors.global_permissions}
					<Field.Error>{fieldErrors.global_permissions}</Field.Error>
				{:else}
					<Field.Description>{t('admin.roles.field.globalHint')}</Field.Description>
				{/if}
			</Field.Field>

			<Field.Field data-invalid={!!fieldErrors.servers}>
				<Field.Label>{t('admin.roles.field.servers')}</Field.Label>
				{#if servers.list.length === 0}
					<Field.Description>{t('admin.roles.field.noServers')}</Field.Description>
				{:else}
					<div class="space-y-4">
						{#each servers.list as entry (entry.id)}
							<div class="rounded-lg border p-3">
								<p class="mb-2 text-sm font-medium">{entry.name}</p>
								<div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
									{#each SERVER_PERMISSIONS as permission (permission)}
										<Label class="text-xs font-normal">
											<Checkbox
												checked={(grants[entry.id] ?? []).includes(permission)}
												onCheckedChange={(on) => toggleServer(entry.id, permission, on)}
											/>
											{t(`common.permission.${permission}`)}
										</Label>
									{/each}
								</div>
							</div>
						{/each}
					</div>
				{/if}
				{#if fieldErrors.servers}
					<Field.Error>{fieldErrors.servers}</Field.Error>
				{:else}
					<Field.Description>{t('admin.roles.field.serversHint')}</Field.Description>
				{/if}
			</Field.Field>
		</div>

		<Dialog.Footer>
			<Button variant="outline" onclick={close} disabled={saving}>
				{t('common.actions.cancel')}
			</Button>
			<Button onclick={save} disabled={saving || !name.trim()}>
				{#if saving}<Spinner class="size-4" />{/if}
				{saving ? t('common.actions.saving') : t('common.actions.save')}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
