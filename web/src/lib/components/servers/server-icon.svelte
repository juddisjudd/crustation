<script lang="ts">
	import PickaxeIcon from '@lucide/svelte/icons/pickaxe';
	import BlocksIcon from '@lucide/svelte/icons/blocks';
	import SwordsIcon from '@lucide/svelte/icons/swords';
	import Gamepad2Icon from '@lucide/svelte/icons/gamepad-2';
	import { cn } from '$lib/utils.js';

	let {
		type,
		icon,
		class: className
	}: { type: string; icon?: string | false | null; class?: string } = $props();

	const Fallback = $derived(
		type === 'minecraft-bedrock'
			? BlocksIcon
			: type === 'hytale'
				? SwordsIcon
				: type === 'steam_cmd'
					? Gamepad2Icon
					: PickaxeIcon
	);
	const src = $derived(icon ? `data:image/png;base64,${icon.replace(/\s/g, '')}` : null);
</script>

<div
	class={cn(
		'grid size-9 shrink-0 place-items-center overflow-hidden rounded-md border bg-muted text-muted-foreground',
		className
	)}
>
	{#if src}
		<img {src} alt="" class="size-full [image-rendering:pixelated]" />
	{:else}
		<Fallback class="size-4" />
	{/if}
</div>
