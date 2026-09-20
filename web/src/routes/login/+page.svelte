<script lang="ts">
	import { base } from '$app/paths';
	import { page } from '$app/state';
	import LogoMark from '$lib/components/brand/logo-mark.svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import * as Field from '$lib/components/ui/field/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { api, ApiError } from '$lib/api/client';

	let username = $state('');
	let password = $state('');
	let pending = $state(false);
	let error = $state<string | null>(null);
	let cooldown = $state(0);

	function nextUrl() {
		const next = page.url.searchParams.get('next');
		return next && next.startsWith(base) ? next : `${base}/`;
	}

	$effect(() => {
		if (cooldown <= 0) return;
		const timer = setInterval(() => (cooldown = Math.max(0, cooldown - 1)), 1000);
		return () => clearInterval(timer);
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		if (pending || cooldown > 0) return;
		pending = true;
		error = null;
		try {
			await api.post('/auth/login', { username, password }, { allowAnonymous: true });
			location.replace(nextUrl());
		} catch (err) {
			if (err instanceof ApiError && err.code === 'RATE_LIMITED') {
				cooldown = err.retryAfter ?? 60;
				error = 'Too many attempts. Wait a moment and try again.';
			} else if (err instanceof ApiError && err.status === 401) {
				error = 'That username and password do not match.';
			} else if (err instanceof ApiError) {
				error = err.message;
			} else {
				error = 'Could not reach the panel. Check that it is running.';
			}
		} finally {
			pending = false;
		}
	}
</script>

<svelte:head><title>Sign in · Crustation</title></svelte:head>

<main class="flex min-h-svh flex-col items-center justify-center px-4 py-12">
	<div class="w-full max-w-[340px]">
		<LogoMark class="mx-auto mb-8 h-8 w-auto" />
		<h1 class="mb-8 text-center text-2xl font-semibold tracking-tight">Sign in to Crustation</h1>

		<form onsubmit={submit} class="space-y-4">
			<Field.Field>
				<Field.Label for="username">Username</Field.Label>
				<Input id="username" autocomplete="username" required bind:value={username} autofocus />
			</Field.Field>
			<Field.Field>
				<Field.Label for="password">Password</Field.Label>
				<Input
					id="password"
					type="password"
					autocomplete="current-password"
					required
					bind:value={password}
				/>
			</Field.Field>
			{#if error}
				<p class="text-sm text-destructive" role="alert">{error}</p>
			{/if}
			<Button type="submit" class="w-full" disabled={pending || cooldown > 0}>
				{#if pending}<Spinner />{/if}
				{cooldown > 0 ? `Wait ${cooldown}s` : 'Continue'}
			</Button>
		</form>

		<p class="mt-8 text-center text-xs text-muted-foreground">
			First time? The password is in <code class="font-mono">config/first-login.txt</code>.
		</p>
	</div>
</main>
