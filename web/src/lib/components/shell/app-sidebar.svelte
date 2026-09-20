<script lang="ts">
	import LayoutGridIcon from '@lucide/svelte/icons/layout-grid';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import UsersIcon from '@lucide/svelte/icons/users';
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import LogoMark from '$lib/components/brand/logo-mark.svelte';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import { Kbd } from '$lib/components/ui/kbd/index.js';
	import NavUser from './nav-user.svelte';
	import StatusDot from './status-dot.svelte';
	import { servers } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { commandMenu } from './command-menu.svelte';
	import { t } from '$lib/i18n/index.svelte';

	const path = $derived(page.url.pathname);
	const isActive = (href: string, exact = false) =>
		exact ? path === href || path === href + '/' : path === href || path.startsWith(href + '/');
</script>

<Sidebar.Root variant="inset" collapsible="icon">
	<Sidebar.Header>
		<Sidebar.Menu>
			<Sidebar.MenuItem>
				<Sidebar.MenuButton size="lg" class="hover:bg-transparent">
					{#snippet child({ props })}
						<a href={resolve('/')} {...props}>
							<!-- Square slot, so the mark's left edge lands where the icons below start. -->
							<LogoMark class="size-8 shrink-0 object-contain" />
							<span class="font-semibold tracking-tight">Crustation</span>
						</a>
					{/snippet}
				</Sidebar.MenuButton>
			</Sidebar.MenuItem>
			<Sidebar.MenuItem>
				<Sidebar.MenuButton
					variant="outline"
					class="text-muted-foreground"
					tooltipContent={t('nav.search')}
					onclick={() => commandMenu.toggle()}
				>
					<SearchIcon />
					<span>{t('nav.search')}</span>
					<Kbd class="ml-auto group-data-[collapsible=icon]:hidden">⌘K</Kbd>
				</Sidebar.MenuButton>
			</Sidebar.MenuItem>
		</Sidebar.Menu>
	</Sidebar.Header>

	<Sidebar.Content>
		<Sidebar.Group>
			<Sidebar.Menu>
				<Sidebar.MenuItem>
					<Sidebar.MenuButton
						isActive={isActive(resolve('/'), true)}
						tooltipContent={t('nav.overview')}
					>
						{#snippet child({ props })}
							<a href={resolve('/')} {...props}>
								<LayoutGridIcon /><span>{t('nav.overview')}</span>
							</a>
						{/snippet}
					</Sidebar.MenuButton>
				</Sidebar.MenuItem>
			</Sidebar.Menu>
		</Sidebar.Group>

		<Sidebar.Group>
			<Sidebar.GroupLabel>{t('nav.servers')}</Sidebar.GroupLabel>
			<Sidebar.Menu>
				{#if session.can('CREATE_SERVER')}
					{@const href = resolve('/servers/new')}
					<Sidebar.MenuItem>
						<Sidebar.MenuButton isActive={isActive(href)} tooltipContent={t('nav.newServer')}>
							{#snippet child({ props })}
								<a {href} {...props}>
									<PlusIcon /><span>{t('nav.newServer')}</span>
								</a>
							{/snippet}
						</Sidebar.MenuButton>
					</Sidebar.MenuItem>
				{/if}
				{#each servers.list as server (server.id)}
					{@const href = resolve(`/servers/${server.id}`)}
					<Sidebar.MenuItem>
						<Sidebar.MenuButton isActive={isActive(href)} tooltipContent={server.name}>
							{#snippet child({ props })}
								<a {href} {...props}>
									<StatusDot status={servers.statusOf(server.id)} class="mx-1" />
									<span>{server.name}</span>
								</a>
							{/snippet}
						</Sidebar.MenuButton>
					</Sidebar.MenuItem>
				{:else}
					{#if servers.loaded}
						<p class="px-2 py-1 text-xs text-muted-foreground group-data-[collapsible=icon]:hidden">
							{t('nav.noServers')}
						</p>
					{:else}
						{#each [0, 1, 2] as i (i)}
							<Sidebar.MenuSkeleton showIcon />
						{/each}
					{/if}
				{/each}
			</Sidebar.Menu>
		</Sidebar.Group>
		{#if session.can('MANAGE_USERS') || session.can('MANAGE_ROLES')}
			<Sidebar.Group>
				<Sidebar.GroupLabel>{t('nav.groupAdmin')}</Sidebar.GroupLabel>
				<Sidebar.Menu>
					{#if session.can('MANAGE_USERS')}
						{@const href = resolve('/users')}
						<Sidebar.MenuItem>
							<Sidebar.MenuButton isActive={isActive(href)} tooltipContent={t('nav.users')}>
								{#snippet child({ props })}
									<a {href} {...props}>
										<UsersIcon /><span>{t('nav.users')}</span>
									</a>
								{/snippet}
							</Sidebar.MenuButton>
						</Sidebar.MenuItem>
					{/if}
					{#if session.can('MANAGE_ROLES')}
						{@const href = resolve('/roles')}
						<Sidebar.MenuItem>
							<Sidebar.MenuButton isActive={isActive(href)} tooltipContent={t('nav.roles')}>
								{#snippet child({ props })}
									<a {href} {...props}>
										<ShieldIcon /><span>{t('nav.roles')}</span>
									</a>
								{/snippet}
							</Sidebar.MenuButton>
						</Sidebar.MenuItem>
					{/if}
				</Sidebar.Menu>
			</Sidebar.Group>
		{/if}
	</Sidebar.Content>

	<Sidebar.Footer>
		{#if session.user}<NavUser user={session.user} />{/if}
	</Sidebar.Footer>
	<Sidebar.Rail />
</Sidebar.Root>
