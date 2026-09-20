<script lang="ts">
	import StatusDot from '$lib/components/shell/status-dot.svelte';
	import { statusLabel, type ServerStatus } from '$lib/servers.svelte';

	let { status }: { status: ServerStatus } = $props();
</script>

<span
	class="inline-flex h-6 items-center gap-1.5 rounded-full border bg-background px-2.5 text-xs font-medium"
>
	<StatusDot {status} />
	{statusLabel(status)}
</span>
