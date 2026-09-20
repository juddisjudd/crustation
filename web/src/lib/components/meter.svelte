<script lang="ts">
	import { cn } from '$lib/utils.js';
	import { clampPercent } from '$lib/format';

	let {
		value,
		label,
		tone = 'usage',
		class: className
	}: {
		value: number | null | undefined;
		label: string;
		/** 'usage' warns as the bar fills; 'neutral' keeps one color. */
		tone?: 'usage' | 'neutral';
		class?: string;
	} = $props();

	const pct = $derived(clampPercent(value));
</script>

<div
	class={cn('h-1 w-full overflow-hidden rounded-full bg-muted', className)}
	role="meter"
	aria-label={label}
	aria-valuemin={0}
	aria-valuemax={100}
	aria-valuenow={Math.round(pct)}
>
	<div
		class={cn('h-full rounded-full transition-[width] duration-500 ease-out', {
			'bg-foreground/80': tone === 'neutral' || pct < 60,
			'bg-warning': tone === 'usage' && pct >= 60 && pct < 85,
			'bg-destructive': tone === 'usage' && pct >= 85
		})}
		style:width="{pct}%"
	></div>
</div>
