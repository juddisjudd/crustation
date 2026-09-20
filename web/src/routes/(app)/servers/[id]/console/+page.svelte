<script lang="ts">
	import ArrowDownIcon from '@lucide/svelte/icons/arrow-down';
	import CornerDownLeftIcon from '@lucide/svelte/icons/corner-down-left';
	import ChevronRightIcon from '@lucide/svelte/icons/chevron-right';
	import ClockIcon from '@lucide/svelte/icons/clock';
	import DownloadIcon from '@lucide/svelte/icons/download';
	import EraserIcon from '@lucide/svelte/icons/eraser';
	import PaletteIcon from '@lucide/svelte/icons/palette';
	import RegexIcon from '@lucide/svelte/icons/regex';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import SearchIcon from '@lucide/svelte/icons/search';
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { tick } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as InputGroup from '$lib/components/ui/input-group/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import { api } from '$lib/api/client';
	import { enableRcon, errorMessage, rconStatus, sendCommand } from '$lib/api/servers';
	import { socket } from '$lib/realtime/socket.svelte';
	import { servers } from '$lib/servers.svelte';
	import type { ConsoleLine, RconStatus } from '$lib/api/types';
	import { createMacro, deleteMacro, listMacros, type Macro } from '$lib/api/macros';
	import { asText, badRegex, levelOf, matcherFor, pieces, plain, type Level } from '$lib/console';
	import { t } from '$lib/i18n/index.svelte';

	const MAX_LINES = 2000;

	const STREAM_STYLE: Record<ConsoleLine['stream'], string> = {
		stdout: '',
		stderr: 'text-[#ff6166]',
		command: 'text-white/45',
		rcon: 'text-[#7fd1b9]'
	};

	let { data } = $props();

	const server = $derived(data.server);
	const running = $derived(servers.isRunning(server.id));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));
	const canConfigure = $derived(server.permissions.includes('CONFIG'));

	let lines = $state.raw<ConsoleLine[]>([]);
	let viewport = $state<HTMLDivElement | null>(null);
	let pinned = $state(true);
	let command = $state('');
	let sending = $state(false);
	let history: string[] = [];
	let historyIndex = -1;
	let rcon = $state<RconStatus | null>(null);
	let enabling = $state(false);

	const LEVELS: Level[] = ['error', 'warn', 'info', 'other'];
	let hidden = $state<Level[]>([]);
	let query = $state('');
	let asRegex = $state(false);
	let timestamps = $state(remembered('timestamps', false));
	let syntax = $state(remembered('syntax', true));

	// Two view switches worth keeping between visits; nothing else is stored,
	// and a window that refuses storage just forgets them.
	function remembered(name: string, fallback: boolean) {
		try {
			const held = localStorage.getItem(`crustation.console.${name}`);
			return held === null ? fallback : held === 'true';
		} catch {
			return fallback;
		}
	}

	$effect(() => {
		try {
			localStorage.setItem('crustation.console.syntax', String(syntax));
			localStorage.setItem('crustation.console.timestamps', String(timestamps));
		} catch {
			// Nothing to do: the view works either way.
		}
	});

	const broken = $derived(badRegex(query, asRegex));
	const matcher = $derived(broken ? null : matcherFor(query, asRegex));
	const filtering = $derived(hidden.length > 0 || !!matcher);

	const shown = $derived.by(() => {
		if (!filtering) return lines;
		return lines.filter((line) => {
			if (hidden.includes(levelOf(line))) return false;
			// Search on the text a person sees, not the colour codes behind it.
			return !matcher || matcher.test(plain(line.text));
		});
	});

	function toggleLevel(level: Level) {
		hidden = hidden.includes(level) ? hidden.filter((one) => one !== level) : [...hidden, level];
	}

	function download() {
		const blob = new Blob([asText(lines)], { type: 'text/plain;charset=utf-8' });
		const url = URL.createObjectURL(blob);
		const link = document.createElement('a');
		link.href = url;
		link.download = `${server.name.replace(/[^\w.-]+/g, '-')}-console.txt`;
		link.click();
		URL.revokeObjectURL(url);
	}

	function clockOf(line: ConsoleLine) {
		return new Date(line.at).toLocaleTimeString();
	}

	let macros = $state.raw<Macro[]>([]);
	let addingMacro = $state(false);
	let macroLabel = $state('');
	let macroCommand = $state('');

	$effect(() => {
		const id = server.id;
		if (!canCommand) return;
		listMacros(id)
			.then((saved) => {
				if (server.id === id) macros = saved;
			})
			.catch(() => {});
	});

	async function runMacro(saved: Macro) {
		try {
			await sendCommand(server.id, saved.command);
			scrollToBottom();
		} catch (err) {
			toast.error(t('server.console.sendError'), { description: errorMessage(err) });
		}
	}

	async function saveMacro() {
		if (!macroLabel.trim() || !macroCommand.trim()) return;
		try {
			await createMacro(server.id, macroLabel.trim(), macroCommand.trim());
			toast.success(t('server.console.macros.saved', { name: macroLabel.trim() }));
			macroLabel = '';
			macroCommand = '';
			addingMacro = false;
			macros = await listMacros(server.id);
		} catch (err) {
			toast.error(t('server.console.sendError'), { description: errorMessage(err) });
		}
	}

	async function dropMacro(saved: Macro) {
		try {
			await deleteMacro(server.id, saved.id);
			macros = await listMacros(server.id);
		} catch (err) {
			toast.error(t('server.console.sendError'), { description: errorMessage(err) });
		}
	}

	// Reachability is proven by connecting, so re-check when the server comes up.
	const rconProbe = $derived({ id: server.id, running });

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
		const { id } = rconProbe;
		let cancelled = false;
		rconStatus(id)
			.then((status) => {
				if (!cancelled) rcon = status;
			})
			.catch(() => {
				if (!cancelled) rcon = null;
			});
		return () => {
			cancelled = true;
		};
	});

	async function turnOnRcon() {
		enabling = true;
		try {
			const result = await enableRcon(server.id);
			rcon = await rconStatus(server.id);
			toast.success(
				result.restart_required
					? t('server.console.rconEnabledRestart')
					: t('server.console.rconEnabledNow', { port: String(result.port) })
			);
		} catch (err) {
			toast.error(t('server.console.rconError'), { description: errorMessage(err) });
		} finally {
			enabling = false;
		}
	}

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
		class="terminal relative flex h-[calc(100svh-16rem)] min-h-90 flex-col overflow-hidden rounded-lg border bg-[#0a0a0a] text-[#ededed] shadow-xs"
	>
		<div class="flex h-10 items-center gap-2 border-b border-white/10 px-3 text-xs text-white/60">
			<span class="font-mono">{server.name}</span>
			<span class="ml-auto inline-flex items-center gap-1.5">
				<span class={['size-1.5 rounded-full', running ? 'bg-success' : 'bg-white/30']}></span>
				{running ? t('server.console.live') : t('server.console.offline')}
			</span>
			{#if rcon?.supported}
				{@const status = rcon}
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							{#if status.enabled}
								<span
									{...props}
									class="rounded border border-white/10 px-1.5 py-0.5 text-[10px] tracking-wide uppercase"
								>
									{t('server.console.rconOn')}
								</span>
							{:else if canConfigure}
								<Button
									{...props}
									variant="ghost"
									size="sm"
									class="h-6 px-2 text-[11px] text-white/60 hover:bg-white/10 hover:text-white"
									disabled={enabling}
									onclick={turnOnRcon}
								>
									{#if enabling}<Spinner />{/if}
									{t('server.console.rconOff')}
								</Button>
							{/if}
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>
						{status.enabled ? t('server.console.rconOnHint') : t('server.console.rconOffHint')}
					</Tooltip.Content>
				</Tooltip.Root>
			{/if}
			{@render iconButton(
				ClockIcon,
				t('server.console.timestamps'),
				() => (timestamps = !timestamps),
				timestamps
			)}
			{@render iconButton(
				PaletteIcon,
				t('server.console.syntax'),
				() => (syntax = !syntax),
				syntax
			)}
			{@render iconButton(DownloadIcon, t('server.console.download'), download)}
			{@render iconButton(EraserIcon, t('server.console.clear'), () => (lines = []))}
		</div>

		<div
			class="flex flex-wrap items-center gap-2 border-b border-white/10 px-3 py-2 text-xs text-white/60"
		>
			{#each LEVELS as level (level)}
				{@const off = hidden.includes(level)}
				<button
					class={[
						'rounded-full border px-2 py-0.5 text-[11px] transition-colors',
						off
							? 'border-white/10 text-white/30 line-through'
							: 'border-white/20 text-white/80 hover:bg-white/10'
					]}
					aria-pressed={!off}
					onclick={() => toggleLevel(level)}
				>
					{t(`server.console.level.${level}`)}
				</button>
			{/each}

			<div class="ml-auto flex items-center gap-1.5">
				<InputGroup.Root class="h-7 w-48 border-white/15 bg-transparent">
					<InputGroup.Addon><SearchIcon class="size-3.5 text-white/40" /></InputGroup.Addon>
					<InputGroup.Input
						bind:value={query}
						placeholder={t('server.console.search')}
						aria-label={t('server.console.search')}
						aria-invalid={broken}
						class="text-[11px] text-white placeholder:text-white/30"
					/>
				</InputGroup.Root>
				{@render iconButton(
					RegexIcon,
					t('server.console.regex'),
					() => (asRegex = !asRegex),
					asRegex
				)}
			</div>

			{#if broken}
				<span class="w-full text-[11px] text-[#ff6166]">{t('server.console.badRegex')}</span>
			{:else if filtering}
				<span class="w-full text-[11px] text-white/40">
					{t('server.console.filtered', { shown: shown.length, total: lines.length })}
				</span>
			{/if}
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
			{#if lines.length > 0 && shown.length === 0}
				<p class="text-white/40">{t('server.console.noMatch')}</p>
			{/if}
			{#each shown as line (line.seq)}
				<div class={['break-words whitespace-pre-wrap', STREAM_STYLE[line.stream]]}>
					{#if timestamps}<span class="text-white/30">{clockOf(line)} </span>{/if}
					{#each pieces(line.text, matcher, syntax && line.stream === 'stdout') as piece, index (index)}
						<span
							class={[
								piece.tone,
								piece.bold && 'font-bold',
								piece.italic && 'italic',
								piece.underline && 'underline',
								piece.strike && 'line-through',
								piece.hit && 'rounded-sm bg-[#ffd866] text-black'
							]}
							style={piece.colour ? `color: ${piece.colour}` : undefined}
						>
							{piece.text}
						</span>
					{:else}
						{' '}
					{/each}
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
			<div
				class="flex flex-wrap items-center gap-1.5 border-t border-white/10 px-2 pt-2"
				aria-label={t('server.console.macros.hint')}
			>
				{#each macros as saved (saved.id)}
					<span class="group inline-flex items-center rounded-full border border-white/15">
						<button
							class="rounded-l-full px-2.5 py-1 text-[11px] text-white/80 hover:bg-white/10"
							title={saved.command}
							onclick={() => runMacro(saved)}
						>
							{saved.label}
						</button>
						{#if canConfigure}
							<button
								class="rounded-r-full px-1.5 py-1 text-white/30 hover:bg-white/10 hover:text-white"
								aria-label={t('server.console.macros.remove', { name: saved.label })}
								onclick={() => dropMacro(saved)}
							>
								<XIcon class="size-3" />
							</button>
						{/if}
					</span>
				{/each}

				{#if canConfigure}
					{#if addingMacro}
						<input
							bind:value={macroLabel}
							placeholder={t('server.console.macros.label')}
							aria-label={t('server.console.macros.label')}
							class="h-7 w-28 rounded border border-white/15 bg-transparent px-2 text-[11px] text-white placeholder:text-white/30"
						/>
						<input
							bind:value={macroCommand}
							placeholder={t('server.console.macros.command')}
							aria-label={t('server.console.macros.command')}
							class="h-7 w-44 rounded border border-white/15 bg-transparent px-2 font-mono text-[11px] text-white placeholder:text-white/30"
						/>
						<Button
							size="sm"
							class="h-7 text-[11px]"
							disabled={!macroLabel.trim() || !macroCommand.trim()}
							onclick={saveMacro}
						>
							{t('common.actions.save')}
						</Button>
						<Button
							size="sm"
							variant="ghost"
							class="h-7 text-[11px] text-white/60 hover:bg-white/10 hover:text-white"
							onclick={() => (addingMacro = false)}
						>
							{t('common.actions.cancel')}
						</Button>
					{:else}
						<button
							class="inline-flex items-center gap-1 rounded-full border border-dashed border-white/15 px-2.5 py-1 text-[11px] text-white/50 hover:bg-white/10 hover:text-white"
							onclick={() => (addingMacro = true)}
						>
							<PlusIcon class="size-3" />
							{t('server.console.macros.add')}
						</button>
					{/if}
				{/if}
			</div>

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

{#snippet iconButton(Icon: typeof EraserIcon, label: string, onclick: () => void, active = false)}
	<Tooltip.Root>
		<Tooltip.Trigger>
			{#snippet child({ props })}
				<Button
					{...props}
					variant="ghost"
					size="icon-xs"
					class={[
						'hover:bg-white/10 hover:text-white',
						active ? 'bg-white/10 text-white' : 'text-white/60'
					]}
					aria-label={label}
					aria-pressed={active}
					{onclick}
				>
					<Icon />
				</Button>
			{/snippet}
		</Tooltip.Trigger>
		<Tooltip.Content>{label}</Tooltip.Content>
	</Tooltip.Root>
{/snippet}
