<script lang="ts">
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import RustConvars from './rust-convars.svelte';
	import RustCommands from './rust-commands.svelte';
	import RustItems from './rust-items.svelte';
	import { t } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);

	const VIEWS = ['convars', 'commands', 'items'] as const;
	const view = $derived(VIEWS.find((one) => one === page.url.searchParams.get('view')) ?? VIEWS[0]);

	function choose(next: string) {
		goto(`?view=${next}`, { replaceState: true, noScroll: true, keepFocus: true });
	}
</script>

<svelte:head><title>{t('nav.tabs.admin')} · {server.name} · Crustation</title></svelte:head>

<div class="page space-y-4 py-4">
	<Tabs.Root value={view} onValueChange={choose}>
		<Tabs.List>
			{#each VIEWS as one (one)}
				<Tabs.Trigger value={one}>{t(`rustAdmin.views.${one}`)}</Tabs.Trigger>
			{/each}
		</Tabs.List>
	</Tabs.Root>

	{#if view === 'convars'}
		<RustConvars {server} />
	{:else if view === 'commands'}
		<RustCommands {server} />
	{:else}
		<RustItems {server} />
	{/if}
</div>
