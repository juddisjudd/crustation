<script lang="ts">
	import GameLogo from '$lib/components/servers/game-logo.svelte';
	import StatusDot from '$lib/components/shell/status-dot.svelte';
	import type { ServerStatus } from '$lib/servers.svelte';
	import { isMinecraft } from '$lib/format';
	import { cn } from '$lib/utils.js';

	interface Props {
		kind: string;
		/** Puts the status dot on the corner, so a list reads without a separate column. */
		status?: ServerStatus;
		/** A bare mark for menus, or the mark on a tile for cards and headers. */
		tile?: boolean;
		/** Matches the surface behind it, so the dot is cut cleanly out of the mark. */
		ring?: string;
		class?: string;
	}

	let { kind, status, tile = false, ring = 'ring-background', class: className }: Props = $props();

	const game = $derived(isMinecraft(kind) ? 'minecraft' : kind);
</script>

<span
	class={cn(
		'relative inline-flex shrink-0',
		tile ? 'size-9 items-center justify-center rounded-md bg-muted text-foreground/80' : 'size-4',
		className
	)}
>
	<GameLogo {game} class={tile ? 'size-5' : 'size-full'} />
	{#if status}
		<StatusDot
			{status}
			class={cn('absolute -right-0.5 -bottom-0.5 ring-2', ring, tile ? 'size-2.5' : 'size-2')}
		/>
	{/if}
</span>
