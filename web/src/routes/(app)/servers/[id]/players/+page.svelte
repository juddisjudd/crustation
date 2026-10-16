<script lang="ts">
	import MinecraftPlayers from './minecraft-players.svelte';
	import RustPlayers from './rust-players.svelte';
	import SteamPlayers from './steam-players.svelte';
	import { countsFromConsole } from '$lib/format';

	let { data } = $props();
</script>

{#if data.server.kind === 'rust'}
	<RustPlayers {data} />
{:else if countsFromConsole(data.server.kind)}
	<SteamPlayers {data} />
{:else}
	<MinecraftPlayers {data} />
{/if}
