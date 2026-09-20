<script lang="ts">
	import type { Snippet } from 'svelte';
	import { cn } from '$lib/utils.js';

	let {
		title,
		description,
		children,
		hint,
		action,
		destructive = false,
		onsubmit,
		class: className
	}: {
		title: string;
		description?: string | Snippet;
		children?: Snippet;
		hint?: string | Snippet;
		action?: Snippet;
		destructive?: boolean;
		onsubmit?: (event: SubmitEvent) => void;
		class?: string;
	} = $props();

	const id = $props.id();
</script>

<form
	class={cn(
		'overflow-hidden rounded-lg border bg-card',
		destructive && 'border-destructive/40',
		className
	)}
	aria-labelledby="{id}-title"
	onsubmit={(event) => {
		event.preventDefault();
		onsubmit?.(event);
	}}
>
	<div class="space-y-4 p-6">
		<div class="space-y-1.5">
			<h2 id="{id}-title" class="text-base font-semibold tracking-tight">{title}</h2>
			{#if description}
				<div class="text-sm text-muted-foreground">
					{#if typeof description === 'string'}{description}{:else}{@render description()}{/if}
				</div>
			{/if}
		</div>
		{@render children?.()}
	</div>
	{#if hint || action}
		<div
			class={cn(
				'flex min-h-14 flex-wrap items-center justify-between gap-3 border-t px-6 py-3 text-sm text-muted-foreground',
				destructive ? 'border-destructive/40 bg-destructive/5' : 'bg-muted/50'
			)}
		>
			<div class="min-w-0">
				{#if typeof hint === 'string'}{hint}{:else}{@render hint?.()}{/if}
			</div>
			{#if action}
				<div class="ml-auto flex items-center gap-2">{@render action()}</div>
			{/if}
		</div>
	{/if}
</form>
