<script lang="ts">
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import AppSidebar from '$lib/components/shell/app-sidebar.svelte';
	import SiteHeader from '$lib/components/shell/site-header.svelte';
	import CommandMenu from '$lib/components/shell/command-menu.svelte';
	import ConfirmDialog from '$lib/components/confirm/confirm-dialog.svelte';
	import { socket } from '$lib/realtime/socket.svelte';
	import { servers } from '$lib/servers.svelte';
	import { registerGlobalEvents } from '$lib/realtime/global-events';

	let { children } = $props();

	$effect(() => {
		socket.connect();
		const stopTopics = socket.subscribe(['servers', 'panel']);
		const stopServers = servers.listen();
		const stopGlobal = registerGlobalEvents();
		servers.refresh();
		return () => {
			stopTopics();
			stopServers();
			stopGlobal();
		};
	});
</script>

<svelte:document onvisibilitychange={() => socket.handleVisibility()} />

<Sidebar.Provider>
	<AppSidebar />
	<Sidebar.Inset class="min-w-0 md:border">
		<SiteHeader />
		<div class="flex flex-1 flex-col">
			{@render children()}
		</div>
	</Sidebar.Inset>
</Sidebar.Provider>
<CommandMenu />
<ConfirmDialog />
