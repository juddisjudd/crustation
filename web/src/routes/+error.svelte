<script lang="ts">
	import { page } from '$app/state';
	import { resolve } from '$app/paths';
	import LogoMark from '$lib/components/brand/logo-mark.svelte';
	import { Button } from '$lib/components/ui/button/index.js';

	const status = $derived(page.status);
	const message = $derived(
		status === 404
			? 'We could not find that page.'
			: (page.error?.message ?? 'Something went wrong.')
	);
</script>

<svelte:head><title>{status} · Crustation</title></svelte:head>

<main class="flex min-h-svh flex-col items-center justify-center gap-6 px-4 text-center">
	<LogoMark class="h-10 w-auto" />
	<div class="space-y-2">
		<p class="font-mono text-sm text-muted-foreground">{status}</p>
		<h1 class="text-xl font-semibold tracking-tight">{message}</h1>
	</div>
	<div class="flex gap-2">
		<Button variant="outline" onclick={() => history.back()}>Go back</Button>
		<Button href={resolve('/')}>Overview</Button>
	</div>
</main>
