<script lang="ts">
	import { cn } from '$lib/utils.js';
	import { statusLabel, type ServerStatus } from '$lib/servers.svelte';

	let { status, class: className }: { status: ServerStatus; class?: string } = $props();

	const busy = $derived(status === 'starting' || status === 'stopping' || status === 'installing');
</script>

<span
	class={cn('relative inline-flex size-2 shrink-0 rounded-full', className, {
		'bg-success': status === 'running',
		'bg-muted-foreground/40': status === 'stopped' || status === 'unknown',
		'bg-warning': busy,
		'bg-destructive': status === 'crashed'
	})}
	role="img"
	aria-label={statusLabel(status)}
	title={statusLabel(status)}
>
	{#if status === 'running' || busy}
		<span
			class="absolute inset-0 animate-ping rounded-full bg-inherit opacity-40 motion-reduce:hidden"
		></span>
	{/if}
</span>
