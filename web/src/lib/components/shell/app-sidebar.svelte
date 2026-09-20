<script lang="ts">
	import LayoutGridIcon from '@lucide/svelte/icons/layout-grid';
	import SearchIcon from '@lucide/svelte/icons/search';
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
							<div class="grid size-8 place-items-center rounded-md border bg-background">
								<LogoMark class="h-5 w-auto" />
							</div>
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
	</Sidebar.Content>

	<Sidebar.Footer>
		{#if session.user}<NavUser user={session.user} />{/if}
	</Sidebar.Footer>
	<Sidebar.Rail />
</Sidebar.Root>
