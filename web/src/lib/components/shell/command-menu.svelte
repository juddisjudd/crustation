<script lang="ts" module>
	class CommandMenuState {
		open = $state(false);
		toggle() {
			this.open = !this.open;
		}
	}

	export const commandMenu = new CommandMenuState();
</script>

<script lang="ts">
	import LayoutGridIcon from '@lucide/svelte/icons/layout-grid';
	import PlayIcon from '@lucide/svelte/icons/play';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SquareIcon from '@lucide/svelte/icons/square';
	import SunIcon from '@lucide/svelte/icons/sun';
	import MoonIcon from '@lucide/svelte/icons/moon';
	import MonitorIcon from '@lucide/svelte/icons/monitor';
	import LogOutIcon from '@lucide/svelte/icons/log-out';
	import { setMode } from 'mode-watcher';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import * as Command from '$lib/components/ui/command/index.js';
	import StatusDot from './status-dot.svelte';
	import { servers } from '$lib/servers.svelte';
	import { session } from '$lib/session.svelte';
	import { powerAction } from '$lib/api/servers';
	import { t } from '$lib/i18n/index.svelte';

	function run(action: () => unknown) {
		commandMenu.open = false;
		action();
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'k' && (event.metaKey || event.ctrlKey)) {
			event.preventDefault();
			commandMenu.toggle();
		}
	}
</script>

<svelte:document {onkeydown} />

<Command.Dialog
	bind:open={commandMenu.open}
	title={t('nav.search')}
	description={t('nav.searchPlaceholder')}
>
	<Command.Input placeholder={t('nav.searchPlaceholder')} />
	<Command.List>
		<Command.Empty>{t('nav.noResults')}</Command.Empty>

		{#if servers.list.length}
			<Command.Group heading={t('nav.groupServers')}>
				{#each servers.list as server (server.id)}
					<Command.Item
						value={`server ${server.name} ${server.id}`}
						onSelect={() => run(() => goto(resolve(`/servers/${server.id}`)))}
					>
						<StatusDot status={servers.statusOf(server.id)} class="mx-1" />
						<span>{server.name}</span>
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}

		<Command.Group heading={t('nav.groupPages')}>
			<Command.Item value="overview" onSelect={() => run(() => goto(resolve('/')))}>
				<LayoutGridIcon />
				<span>{t('nav.overview')}</span>
			</Command.Item>
			{#if session.can('CREATE_SERVER')}
				<Command.Item
					value="new server create"
					onSelect={() => run(() => goto(resolve('/servers/new')))}
				>
					<PlusIcon />
					<span>{t('nav.newServer')}</span>
				</Command.Item>
			{/if}
		</Command.Group>

		{#if servers.list.some((server) => server.permissions.includes('COMMANDS'))}
			<Command.Group heading={t('nav.groupActions')}>
				{#each servers.list.filter( (server) => server.permissions.includes('COMMANDS') ) as server (server.id)}
					{@const running = servers.isRunning(server.id)}
					<Command.Item
						value={`${running ? 'stop' : 'start'} ${server.name}`}
						onSelect={() =>
							run(() => powerAction(server.id, running ? 'stop' : 'start', server.name))}
					>
						{#if running}<SquareIcon />{:else}<PlayIcon />{/if}
						<span>
							{running ? t('common.server.action.stop') : t('common.server.action.start')}
							{server.name}
						</span>
					</Command.Item>
				{/each}
			</Command.Group>
		{/if}

		<Command.Group heading={t('nav.groupPreferences')}>
			<Command.Item value="theme light" onSelect={() => run(() => setMode('light'))}>
				<SunIcon /><span>{t('nav.themeLight')}</span>
			</Command.Item>
			<Command.Item value="theme dark" onSelect={() => run(() => setMode('dark'))}>
				<MoonIcon /><span>{t('nav.themeDark')}</span>
			</Command.Item>
			<Command.Item value="theme system" onSelect={() => run(() => setMode('system'))}>
				<MonitorIcon /><span>{t('nav.themeSystem')}</span>
			</Command.Item>
			<Command.Item value="sign out" onSelect={() => run(() => session.logout())}>
				<LogOutIcon /><span>{t('nav.logout')}</span>
			</Command.Item>
		</Command.Group>
	</Command.List>
</Command.Dialog>
