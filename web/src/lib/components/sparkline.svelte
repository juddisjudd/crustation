<script lang="ts">
	interface Props {
		/** Newest last. A null is a stretch nothing was sampled in. */
		points: (number | null)[];
		/** What the line is of, for anyone not looking at it. */
		label: string;
		class?: string;
	}

	let { points, label, class: className }: Props = $props();

	const WIDTH = 100;
	const HEIGHT = 24;

	// Always drawn against 0–100, because these are percentages and a line that
	// rescales to its own maximum makes a quiet server look as busy as a loaded
	// one.
	const shape = $derived.by(() => {
		if (points.length < 2) return null;
		const step = WIDTH / (points.length - 1);

		// A gap splits the line rather than joining across it.
		const runs: string[] = [];
		let run: string[] = [];
		points.forEach((value, index) => {
			if (value === null) {
				if (run.length > 1) runs.push(run.join(' '));
				run = [];
				return;
			}
			const x = index * step;
			const y = HEIGHT - (Math.max(0, Math.min(100, value)) / 100) * HEIGHT;
			run.push(`${x.toFixed(1)},${y.toFixed(1)}`);
		});
		if (run.length > 1) runs.push(run.join(' '));
		return runs.length ? runs : null;
	});
</script>

{#if shape}
	<svg
		class={className}
		viewBox="0 0 {WIDTH} {HEIGHT}"
		preserveAspectRatio="none"
		role="img"
		aria-label={label}
	>
		{#each shape as run, index (index)}
			<polyline
				points={run}
				fill="none"
				stroke="currentColor"
				stroke-width="1.5"
				stroke-linecap="round"
				stroke-linejoin="round"
				vector-effect="non-scaling-stroke"
			/>
		{/each}
	</svg>
{/if}
