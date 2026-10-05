<script lang="ts">
	// Icon-set marks (Material Design Icons, Apache-2.0; Simple Icons, CC0). Drop in the
	// publishers' own artwork here if you have it.
	const marks: Record<string, string> = {
		minecraft:
			'M4 2h16a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2m2 4v4h4v2H8v6h2v-2h4v2h2v-6h-2v-2h4V6h-4v4h-4V6z',
		rust: 'M12 0C5.388 0 0 5.388 0 12s5.388 12 12 12s12-5.388 12-12S18.629 0 12 0m0 21.314c-5.133 0-9.297-4.164-9.297-9.297S6.867 2.72 12 2.72s9.297 4.164 9.297 9.297s-4.164 9.297-9.297 9.297M10.028 12l1.48 1.479l-1.922 1.92l-1.478-1.478l-1.428 1.444l-1.92-1.92L6.203 12l-1.377-1.377l1.92-1.904l1.36 1.377l1.411-1.41l1.921 1.903L10.03 12zm9.162-1.462l-1.411 1.411l1.479 1.479l-1.92 1.904l-1.48-1.48l-1.444 1.446l-1.904-1.921l1.445-1.428l-1.377-1.377l1.904-1.92l1.376 1.376l1.411-1.41l1.92 1.92z'
	};

	let { game, class: className }: { game: string; class?: string } = $props();
</script>

<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true" class={className}>
	<path d={marks[game] ?? marks.minecraft} />
</svg>
