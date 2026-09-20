<script lang="ts">
	import ChevronsUpDownIcon from '@lucide/svelte/icons/chevrons-up-down';
	import LogOutIcon from '@lucide/svelte/icons/log-out';
	import SunIcon from '@lucide/svelte/icons/sun';
	import MoonIcon from '@lucide/svelte/icons/moon';
	import MonitorIcon from '@lucide/svelte/icons/monitor';
	import { setMode, userPrefersMode } from 'mode-watcher';
	import * as Avatar from '$lib/components/ui/avatar/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import { session } from '$lib/session.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import type { User } from '$lib/api/types';

	let { user }: { user: User } = $props();

	const sidebar = Sidebar.useSidebar();
	const initials = $derived(user.username.slice(0, 2).toUpperCase());
	const themes = $derived([
		{ value: 'light', label: t('nav.themeLight'), icon: SunIcon },
		{ value: 'dark', label: t('nav.themeDark'), icon: MoonIcon },
		{ value: 'system', label: t('nav.themeSystem'), icon: MonitorIcon }
	] as const);
</script>

<Sidebar.Menu>
	<Sidebar.MenuItem>
		<DropdownMenu.Root>
			<DropdownMenu.Trigger>
				{#snippet child({ props })}
					<Sidebar.MenuButton
						size="lg"
						class="data-[state=open]:bg-sidebar-accent data-[state=open]:text-sidebar-accent-foreground"
						{...props}
					>
						<Avatar.Root class="size-8 rounded-full">
							<Avatar.Fallback class="rounded-full text-xs">{initials}</Avatar.Fallback>
						</Avatar.Root>
						<div class="grid flex-1 text-left text-sm leading-tight">
							<span class="truncate font-medium">{user.username}</span>
							<span class="truncate text-xs text-muted-foreground">
								{user.is_admin ? t('nav.administrator') : t('nav.member')}
							</span>
						</div>
						<ChevronsUpDownIcon class="ml-auto size-4 text-muted-foreground" />
					</Sidebar.MenuButton>
				{/snippet}
			</DropdownMenu.Trigger>
			<DropdownMenu.Content
				class="w-(--bits-dropdown-menu-anchor-width) min-w-60"
				side={sidebar.isMobile ? 'bottom' : 'top'}
				align="start"
				sideOffset={6}
			>
				<DropdownMenu.Label class="font-normal">
					<div class="grid text-sm leading-tight">
						<span class="font-medium">{user.username}</span>
						{#if user.email}
							<span class="truncate text-xs text-muted-foreground">{user.email}</span>
						{/if}
					</div>
				</DropdownMenu.Label>
				<DropdownMenu.Separator />
				<div class="flex items-center justify-between px-2 py-1.5 text-sm">
					<span>{t('nav.theme')}</span>
					<div
						class="flex items-center rounded-full border p-0.5"
						role="radiogroup"
						aria-label={t('nav.theme')}
					>
						{#each themes as theme (theme.value)}
							<button
								type="button"
								role="radio"
								aria-checked={userPrefersMode.current === theme.value}
								aria-label={theme.label}
								title={theme.label}
								class="grid size-6 place-items-center rounded-full text-muted-foreground transition-colors hover:text-foreground aria-checked:bg-accent aria-checked:text-foreground"
								onclick={() => setMode(theme.value)}
							>
								<theme.icon class="size-3.5" />
							</button>
						{/each}
					</div>
				</div>
				<DropdownMenu.Separator />
				<DropdownMenu.Item onSelect={() => session.logout()}>
					<LogOutIcon />
					{t('nav.logout')}
				</DropdownMenu.Item>
			</DropdownMenu.Content>
		</DropdownMenu.Root>
	</Sidebar.MenuItem>
</Sidebar.Menu>
