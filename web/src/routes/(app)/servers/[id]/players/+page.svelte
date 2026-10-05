<script lang="ts">
	import MinecraftPlayers from './minecraft-players.svelte';
	import RustPlayers from './rust-players.svelte';

	let { data } = $props();
</script>

{#if data.server.kind === 'rust'}
	<RustPlayers {data} />
{:else}
	<MinecraftPlayers {data} />
{/if}
