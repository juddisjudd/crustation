<script lang="ts">
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import CornerDownLeftIcon from '@lucide/svelte/icons/corner-down-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import EraserIcon from '@lucide/svelte/icons/eraser';
	import { toast } from 'svelte-sonner';
	import { tick } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as InputGroup from '$lib/components/ui/input-group/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import { api } from '$lib/api/client';
	import { errorMessage, sendCommand } from '$lib/api/servers';
	import { socket } from '$lib/realtime/socket.svelte';
	import { servers } from '$lib/servers.svelte';
	import type { ConsoleLine } from '$lib/api/types';
	import { t } from '$lib/i18n/index.svelte';

	const MAX_LINES = 2000;

	let { data } = $props();

	const server = $derived(data.server);
	const running = $derived(servers.isRunning(server.id));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));

	let lines = $state.raw<ConsoleLine[]>([]);
	let viewport = $state<HTMLDivElement | null>(null);
	let pinned = $state(true);
	let command = $state('');
	let sending = $state(false);
	let history: string[] = [];
	let historyIndex = -1;

	function append(incoming: ConsoleLine[]) {
		if (!incoming.length) return;
		const merged = lines.concat(incoming);
		lines = merged.length > MAX_LINES ? merged.slice(-MAX_LINES) : merged;
		if (pinned) tick().then(scrollToBottom);
	}

	function scrollToBottom() {
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
		lines = [];
		let cancelled = false;
		const buffered: ConsoleLine[] = [];
		let ready = false;

		const stop = socket.on<ConsoleLine>(`server:${id}:console`, 'line', (line) => {
			if (ready) append([line]);
			else buffered.push(line);
		});

		api
			.get<{ lines: ConsoleLine[] }>(`/servers/${id}/console`)
			.then((result) => {
				if (!cancelled) append(result.lines ?? []);
			})
			.catch((err) => {
				if (!cancelled) {
					toast.error(t('server.console.historyError'), { description: errorMessage(err) });
				}
			})
			.finally(() => {
				ready = true;
				const seen = new Set(lines.map((line) => line.seq));
				append(buffered.filter((line) => !seen.has(line.seq)));
			});

		return () => {
			cancelled = true;
			stop();
		};
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const value = command.trim();
		if (!value || sending) return;
		sending = true;
		try {
			await sendCommand(server.id, value);
			if (history[0] !== value) history.unshift(value);
			history = history.slice(0, 100);
			historyIndex = -1;
			command = '';
			scrollToBottom();
		} catch (err) {
			toast.error(t('server.console.sendError'), { description: errorMessage(err) });
		} finally {
			sending = false;
		}
	}

	function onkeydown(event: KeyboardEvent) {
		if (event.key === 'ArrowUp' && history.length) {
			event.preventDefault();
			historyIndex = Math.min(historyIndex + 1, history.length - 1);
			command = history[historyIndex];
		} else if (event.key === 'ArrowDown') {
			event.preventDefault();
			historyIndex = Math.max(historyIndex - 1, -1);
			command = historyIndex === -1 ? '' : history[historyIndex];
		}
	}
</script>

<svelte:head><title>{t('nav.tabs.console')} · {server.name} · Crustation</title></svelte:head>

<div class="mx-auto flex w-full max-w-6xl flex-1 flex-col px-4 py-6 md:px-8">
	<div
		class="terminal relative flex h-[calc(100svh-17rem)] min-h-[360px] flex-col overflow-hidden rounded-lg border bg-[#0a0a0a] text-[#ededed] shadow-xs"
	>
		<div class="flex h-10 items-center gap-2 border-b border-white/10 px-3 text-xs text-white/60">
			<span class="flex gap-1.5" aria-hidden="true">
				<span class="size-2.5 rounded-full bg-white/15"></span>
				<span class="size-2.5 rounded-full bg-white/15"></span>
				<span class="size-2.5 rounded-full bg-white/15"></span>
			</span>
			<span class="ml-2 font-mono">{server.name}</span>
			<span class="ml-auto inline-flex items-center gap-1.5">
				<span class={['size-1.5 rounded-full', running ? 'bg-success' : 'bg-white/30']}></span>
				{running ? t('server.console.live') : t('server.console.offline')}
			</span>
			<Tooltip.Root>
				<Tooltip.Trigger>
					{#snippet child({ props })}
						<Button
							{...props}
							variant="ghost"
							size="icon-xs"
							class="text-white/60 hover:bg-white/10 hover:text-white"
							aria-label={t('server.console.clear')}
							onclick={() => (lines = [])}
						>
							<EraserIcon />
						</Button>
					{/snippet}
				</Tooltip.Trigger>
				<Tooltip.Content>{t('server.console.clear')}</Tooltip.Content>
			</Tooltip.Root>
		</div>

		<div
			bind:this={viewport}
			{onscroll}
			class="min-h-0 flex-1 overflow-y-auto px-4 py-3 font-mono text-[12.5px] leading-5"
			role="log"
			aria-live="off"
			aria-label={t('server.console.output')}
		>
			{#if lines.length === 0}
				<p class="text-white/40">
					{running ? t('server.console.waiting') : t('server.console.stopped')}
				</p>
			{/if}
			{#each lines as line (line.seq)}
				<div
					class={['break-words whitespace-pre-wrap', line.stream === 'stderr' && 'text-[#ff6166]']}
				>
					{line.text || ' '}
				</div>
			{/each}
		</div>

		{#if !pinned}
			<Button
				size="sm"
				variant="secondary"
				class="absolute right-4 bottom-20 shadow-md"
				onclick={scrollToBottom}
			>
				<ArrowDownIcon />
				{t('server.console.latest')}
			</Button>
		{/if}

		{#if canCommand}
			<form onsubmit={submit} class="border-t border-white/10 p-2">
				<InputGroup.Root
					class="border-white/10 bg-white/5 text-white has-[[data-slot=input-group-control]:focus-visible]:border-white/30 has-[[data-slot=input-group-control]:focus-visible]:ring-white/10 dark:bg-white/5"
				>
					<InputGroup.Addon class="text-white/50"><ChevronRightIcon /></InputGroup.Addon>
					<InputGroup.Input
						bind:value={command}
						{onkeydown}
						disabled={!running}
						placeholder={running
							? t('server.console.placeholder')
							: t('server.console.placeholderStopped')}
						class="font-mono text-[13px] placeholder:text-white/35"
						autocomplete="off"
						spellcheck={false}
						aria-label={t('server.console.command')}
					/>
					<InputGroup.Addon align="inline-end">
						<InputGroup.Button
							type="submit"
							size="icon-xs"
							disabled={!running || sending || !command.trim()}
							class="text-white/70 hover:bg-white/10 hover:text-white"
							aria-label={t('server.console.send')}
						>
							<CornerDownLeftIcon />
						</InputGroup.Button>
					</InputGroup.Addon>
				</InputGroup.Root>
			</form>
		{/if}
	</div>
</div>
