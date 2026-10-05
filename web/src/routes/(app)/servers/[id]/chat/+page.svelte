<script lang="ts">
	import MinecraftChat from './minecraft-chat.svelte';
	import RustChat from './rust-chat.svelte';

	let { data } = $props();
</script>

{#if data.server.kind === 'rust'}
	<RustChat {data} />
{:else}
	<MinecraftChat {data} />
{/if}
