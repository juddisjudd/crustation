<script lang="ts">
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import LogInIcon from '@lucide/svelte/icons/log-in';
	import LogOutIcon from '@lucide/svelte/icons/log-out';
	import SendIcon from '@lucide/svelte/icons/send';
	import TrophyIcon from '@lucide/svelte/icons/trophy';
	import { toast } from 'svelte-sonner';
	import { tick } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import { api } from '$lib/api/client';
	import { errorMessage } from '$lib/api/servers';
	import { actOnPlayer, headUrl } from '$lib/api/players';
	import { socket } from '$lib/realtime/socket.svelte';
	import { servers } from '$lib/servers.svelte';
	import type { ConsoleLine } from '$lib/api/types';
	import { chatOf, type ChatEvent } from '$lib/chat';
	import { pieces } from '$lib/console';
	import { t } from '$lib/i18n/index.svelte';

	const MAX_EVENTS = 500;

	let { data } = $props();

	const server = $derived(data.server);
	const running = $derived(servers.isRunning(server.id));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));
	const bedrock = $derived(server.kind === 'minecraft_bedrock');

	let events = $state.raw<ChatEvent[]>([]);
	let viewport = $state<HTMLDivElement | null>(null);
	let pinned = $state(true);
	let message = $state('');
	let sending = $state(false);

	/** Runs of chat from the same person collapse into one block. */
	const blocks = $derived.by(() => {
		const out: { lead: ChatEvent; more: ChatEvent[] }[] = [];
		for (const event of events) {
			const last = out.at(-1);
			const same =
				last &&
				last.lead.kind === event.kind &&
				last.lead.who === event.who &&
				(event.kind === 'chat' || event.kind === 'server') &&
				Date.parse(event.at) - Date.parse(last.more.at(-1)?.at ?? last.lead.at) < 120_000;
			if (same) last.more.push(event);
			else out.push({ lead: event, more: [] });
		}
		return out;
	});

	function append(incoming: ChatEvent[]) {
		if (!incoming.length) return;
		const merged = events.concat(incoming);
		events = merged.length > MAX_EVENTS ? merged.slice(-MAX_EVENTS) : merged;
		if (pinned) tick().then(toBottom);
	}

	function toBottom() {
		viewport?.scrollTo({ top: viewport.scrollHeight });
		pinned = true;
	}

	function onscroll() {
		if (!viewport) return;
		pinned = viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 24;
	}

	$effect(() => socket.subscribe([`server:${server.id}:console`]));

	$effect(() => {
		const id = server.id;
		events = [];
		let cancelled = false;
		const buffered: ChatEvent[] = [];
		let ready = false;

		const stop = socket.on<ConsoleLine>(`server:${id}:console`, 'line', (line) => {
			const found = chatOf(line);
			if (!found) return;
			if (ready) append([found]);
			else buffered.push(found);
		});

		api
			.get<{ lines: ConsoleLine[] }>(`/servers/${id}/console`)
			.then((result) => {
				if (cancelled) return;
				append((result.lines ?? []).map(chatOf).filter((one) => one !== null));
			})
			.catch(() => {})
			.finally(() => {
				ready = true;
				const seen = new Set(events.map((one) => one.seq));
				append(buffered.filter((one) => !seen.has(one.seq)));
			});

		return () => {
			cancelled = true;
			stop();
		};
	});

	async function send(event: SubmitEvent) {
		event.preventDefault();
		const text = message.trim();
		if (!text || sending) return;
		sending = true;
		try {
			await actOnPlayer(server.id, { action: 'say', message: text });
			message = '';
			toBottom();
		} catch (err) {
			toast.error(t('chat.failed'), { description: errorMessage(err) });
		} finally {
			sending = false;
		}
	}

	const clock = (at: string) =>
		new Date(at).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
</script>

<svelte:head><title>{t('nav.tabs.chat')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto flex w-full max-w-4xl flex-1 flex-col gap-4 px-4 py-6 md:px-8">
	{#if bedrock}
		<Alert.Root>
			<Alert.Description>{t('chat.bedrock')}</Alert.Description>
		</Alert.Root>
	{/if}

	<div
		class="relative flex h-[calc(100svh-16rem)] min-h-90 flex-col overflow-hidden rounded-lg border bg-card"
	>
		<div
			bind:this={viewport}
			{onscroll}
			class="min-h-0 flex-1 space-y-3 overflow-y-auto px-4 py-4"
			role="log"
			aria-live="polite"
			aria-label={t('chat.title')}
		>
			{#if blocks.length === 0}
				<p class="py-10 text-center text-sm text-muted-foreground">
					{running ? t('chat.quiet') : t('chat.stopped')}
				</p>
			{/if}

			{#each blocks as block (block.lead.seq)}
				{@const lead = block.lead}
				{#if lead.kind === 'chat' || lead.kind === 'server'}
					<div class="flex gap-2.5">
						{#if lead.kind === 'server'}
							<div
								class="mt-0.5 flex size-7 shrink-0 items-center justify-center rounded bg-primary/10 text-[10px] font-semibold text-primary"
							>
								S
							</div>
						{:else}
							<img
								src={headUrl({ name: lead.who ?? '' })}
								alt=""
								class="mt-0.5 size-7 shrink-0 rounded"
							/>
						{/if}
						<div class="min-w-0 flex-1">
							<div class="flex items-baseline gap-2">
								<span class="text-sm font-medium">
									{lead.kind === 'server' ? t('chat.asServer') : lead.who}
								</span>
								<span class="text-xs text-muted-foreground tabular-nums">{clock(lead.at)}</span>
							</div>
							{#each [lead, ...block.more] as said (said.seq)}
								<p class="text-sm break-words whitespace-pre-wrap">
									{#each pieces(said.text, null) as piece, at (at)}
										<span
											class={[
												piece.bold && 'font-bold',
												piece.italic && 'italic',
												piece.underline && 'underline',
												piece.strike && 'line-through'
											]}
											style={piece.colour ? `color: ${piece.colour}` : undefined}>{piece.text}</span
										>
									{/each}
								</p>
							{/each}
						</div>
					</div>
				{:else}
					<div class="flex items-center gap-2 text-xs text-muted-foreground">
						{#if lead.kind === 'join'}
							<LogInIcon class="size-3.5 text-success" />
							<span>{t('chat.joined', { name: lead.who ?? '' })}</span>
						{:else if lead.kind === 'leave'}
							<LogOutIcon class="size-3.5" />
							<span>{t('chat.left', { name: lead.who ?? '' })}</span>
						{:else if lead.kind === 'advancement'}
							<TrophyIcon class="size-3.5 text-warning" />
							<span>{t('chat.earned', { name: lead.who ?? '', what: lead.text })}</span>
						{:else}
							<span class="italic">* {lead.who} {lead.text}</span>
						{/if}
						<span class="ml-auto tabular-nums">{clock(lead.at)}</span>
					</div>
				{/if}
			{/each}
		</div>

		{#if !pinned}
			<Button
				size="sm"
				variant="secondary"
				class="absolute right-4 bottom-20 shadow-md"
				onclick={toBottom}
			>
				<ArrowDownIcon />
				{t('server.console.latest')}
			</Button>
		{/if}

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
