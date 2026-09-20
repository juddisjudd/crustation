<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '$lib/utils.js';

	let {
		title,
		description,
		actions,
		class: className
	}: {
		title: string;
		description?: string | Snippet;
		actions?: Snippet;
		class?: string;
	} = $props();
</script>

<div class={cn('flex flex-wrap items-end justify-between gap-4', className)}>
	<div class="min-w-0">
		<h1 class="text-2xl font-semibold tracking-tight">{title}</h1>
		{#if description}
			<p class="mt-1 text-sm text-muted-foreground">
				{#if typeof description === 'string'}{description}{:else}{@render description()}{/if}
			</p>
		{/if}
	</div>
	{#if actions}
		<div class="flex flex-wrap items-center gap-2">{@render actions()}</div>
	{/if}
</div>
