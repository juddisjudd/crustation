<script lang="ts">
	import type { SVGAttributes } from 'svelte/elements';

	let { class: className, ...rest }: SVGAttributes<SVGSVGElement> = $props();
</script>

<!-- A shell: two claws around a carapace. -->
<svg viewBox="0 0 32 32" fill="none" aria-hidden="true" class={className} {...rest}>
	<path d="M6 19a10 10 0 0 1 20 0" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
	<path
		d="M6 19c-2.2 0-4-1.8-4-4s1.8-4 4-4"
		stroke="currentColor"
		stroke-width="3"
		stroke-linecap="round"
	/>
	<path
		d="M26 19c2.2 0 4-1.8 4-4s-1.8-4-4-4"
		stroke="currentColor"
		stroke-width="3"
		stroke-linecap="round"
	/>
	<circle cx="12.5" cy="15.5" r="1.75" fill="currentColor" />
	<circle cx="19.5" cy="15.5" r="1.75" fill="currentColor" />
	<path d="M9 24h14" stroke="currentColor" stroke-width="3" stroke-linecap="round" />
</svg>
