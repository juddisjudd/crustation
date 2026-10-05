<script lang="ts">
	import type { PageData } from './$types';
	import SendIcon from '@lucide/svelte/icons/send';
	import { toast } from 'svelte-sonner';
	import { tick } from 'svelte';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { errorMessage } from '$lib/api/servers';
	import { actOnRustPlayer, rustChat, type RustChatLine } from '$lib/api/rust';
	import { socket } from '$lib/realtime/socket.svelte';
	import { servers } from '$lib/servers.svelte';
	import { t } from '$lib/i18n/index.svelte';

	const TEAM = 1;
	const SERVER = 2;

	let { data }: { data: PageData } = $props();

	const server = $derived(data.server);
	const running = $derived(servers.isRunning(server.id));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));

	let lines = $state.raw<RustChatLine[]>([]);
	let viewport = $state<HTMLDivElement | null>(null);
	let message = $state('');
	let sending = $state(false);

	async function show(next: RustChatLine[]) {
		const grew = next.length !== lines.length;
		lines = next;
		if (grew) {
			await tick();
			viewport?.scrollTo({ top: viewport.scrollHeight });
		}
	}

	async function load() {
		try {
			await show((await rustChat(server.id)).lines);
		} catch {
			// the next push or the next look will do
		}
	}

	$effect(() => socket.subscribe([`server:${server.id}:console`]));

	$effect(() => {
		const id = server.id;
		return socket.on<RustChatLine>(`server:${id}:console`, 'chat', (line) => {
			show([...lines, line]);
		});
	});

	$effect(() => {
		void server.id;
		if (!running) return;
		load();
		// Chat arrives as it is said; this only catches anything missed while disconnected.
		const timer = setInterval(load, 30_000);
		return () => clearInterval(timer);
	});

	async function send(event: SubmitEvent) {
		event.preventDefault();
		const text = message.trim();
		if (!text) return;
		sending = true;
		try {
			await actOnRustPlayer(server.id, { action: 'say', message: text });
			message = '';
			await load();
		} catch (err) {
			toast.error(t('chat.failed'), { description: errorMessage(err) });
		} finally {
			sending = false;
		}
	}

	/** Rust chat carries Unity rich-text tags, such as <color=…>, that a browser would print. */
	const plain = (text: string) => text.replace(/<\/?[a-z]+(=[^>]*)?>/gi, '');

	const clock = (time: number) =>
		new Date(time * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
</script>

<svelte:head><title>{t('nav.tabs.chat')} · {server.name} · Crustation</title></svelte:head>

<div class="page flex flex-1 flex-col py-4">
	<div
		class="relative flex h-[calc(100svh-13.5rem)] min-h-90 flex-col overflow-hidden rounded-lg border bg-card"
	>
		<div
			bind:this={viewport}
			class="min-h-0 flex-1 space-y-2 overflow-y-auto px-4 py-4"
			role="log"
			aria-live="polite"
			aria-label={t('chat.title')}
		>
			{#if lines.length === 0}
				<p class="py-10 text-center text-sm text-muted-foreground">
					{running ? t('chat.quiet') : t('chat.stopped')}
				</p>
			{/if}
			{#each lines as line, at (at)}
				<div class="flex items-baseline gap-2 text-sm">
					<span class="shrink-0 text-xs whitespace-nowrap text-muted-foreground tabular-nums">
						{clock(line.time)}
					</span>
					<span class="shrink-0 font-medium">
						{line.channel === SERVER ? t('chat.asServer') : plain(line.username)}
					</span>
					{#if line.channel === TEAM}
						<Badge variant="secondary">{t('chat.team')}</Badge>
					{/if}
					<span class="min-w-0 wrap-break-word whitespace-pre-wrap">{plain(line.message)}</span>
				</div>
			{/each}
		</div>

		{#if canCommand}
			<form onsubmit={send} class="flex gap-2 border-t p-3">
				<Input
					bind:value={message}
					disabled={!running}
					placeholder={running ? t('chat.placeholder') : t('chat.placeholderStopped')}
					aria-label={t('chat.send')}
					autocomplete="off"
				/>
				<Button type="submit" disabled={!running || sending || !message.trim()}>
					<SendIcon />
					{t('chat.send')}
				</Button>
			</form>
		{/if}
	</div>
</div>
