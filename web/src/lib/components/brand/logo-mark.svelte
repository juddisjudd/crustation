<script lang="ts">
	import { base } from '$app/paths';
	import type { HTMLImgAttributes } from 'svelte/elements';

	let { class: className, alt = '', ...rest }: HTMLImgAttributes = $props();
</script>

<img src="{base}/logo.png" {alt} class={className} draggable="false" {...rest} />
