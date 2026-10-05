<script lang="ts">
	import RefreshCwIcon from '@lucide/svelte/icons/refresh-cw';
	import SearchIcon from '@lucide/svelte/icons/search';
	import SendIcon from '@lucide/svelte/icons/send';
	import TerminalIcon from '@lucide/svelte/icons/square-terminal';
	import { untrack } from 'svelte';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Tabs from '$lib/components/ui/tabs/index.js';
	import { rustCommands, type RustCommand } from '$lib/api/rust';
	import { errorMessage, sendCommand } from '$lib/api/servers';
	import type { Server } from '$lib/api/types';
	import { servers } from '$lib/servers.svelte';
	import { t, plural } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	const PAGE = 60;

	/** Server-side commands an admin reaches for, with a start on the arguments where they take some. */
	const COMMON: Record<string, string> = {
		'global.say': 'say ',
		'global.restart': 'restart 300 "Restarting in five minutes"',
		'server.save': 'server.save',
		'server.writecfg': 'server.writecfg',
		'global.status': 'status',
		'inventory.giveall': 'inventory.giveall wood 1000',
		'inventory.clearinventory': 'inventory.clearinventory ',
		'inventory.unlockall': 'inventory.unlockall ',
		'global.teleport': 'teleport ',
		'global.killplayer': 'killplayer ',
		'ai.killanimals': 'ai.killanimals',
		'ai.killscientists': 'ai.killscientists',
		'spawn.fill_groups': 'spawn.fill_groups',
		'spawn.fill_populations': 'spawn.fill_populations',
		'env.addtime': 'env.addtime 6',
		'weather.reset': 'weather.reset'
	};

	let listing = $state.raw<{ running: boolean; live: boolean; commands: RustCommand[] } | null>(
		null
	);
	let failed = $state<string | null>(null);
	let loading = $state(false);
	let query = $state('');
	let mode = $state<'common' | 'all'>('common');
	let open = $state<string | null>(null);
	let line = $state('');
	let sending = $state(false);
	let replies = $state<Record<string, { ok: boolean; text: string }>>({});

	const running = $derived(servers.isLive(server.id));
	const ready = $derived(servers.statusOf(server.id) === 'running');

	async function load() {
		loading = true;
		failed = null;
		try {
			listing = await rustCommands(server.id);
		} catch (error) {
			failed = errorMessage(error);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		void ready;
		void server.id;
		untrack(load);
	});

	const needle = $derived(query.trim().toLowerCase());
	// Back to the first page whenever what is shown changes.
	let limit = $derived.by(() => {
		void needle;
		void mode;
		return PAGE;
	});
	const matches = $derived(
		(listing?.commands ?? []).filter((command) =>
			needle
				? command.name.includes(needle) || command.help.toLowerCase().includes(needle)
				: mode === 'all' || command.name in COMMON
		)
	);
	const visible = $derived(matches.slice(0, limit));

	function toggle(command: RustCommand) {
		if (open === command.name) {
			open = null;
			return;
		}
		open = command.name;
		line = COMMON[command.name] ?? `${command.name} `;
	}

	async function send(name: string) {
		const text = line.trim();
		if (!text) return;
		sending = true;
		try {
			const result = await sendCommand(server.id, text);
			replies[name] = { ok: true, text: result.output?.trim() ?? '' };
		} catch (error) {
			replies[name] = { ok: false, text: errorMessage(error) };
		} finally {
			sending = false;
		}
	}
</script>

<section class="rounded-lg border bg-card">
	<div class="flex flex-wrap items-center gap-2 border-b px-5 py-3">
		<div class="relative w-full sm:w-72">
			<SearchIcon
				class="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground"
			/>
			<Input
				type="search"
				bind:value={query}
				placeholder={t('rustAdmin.search')}
				aria-label={t('rustAdmin.search')}
				onkeydown={(event) => event.key === 'Escape' && (query = '')}
				class="pl-8"
			/>
		</div>
		{#if !needle}
			<Tabs.Root bind:value={mode}>
				<Tabs.List>
					<Tabs.Trigger value="common">{t('rustAdmin.commands.common')}</Tabs.Trigger>
					<Tabs.Trigger value="all">{t('rustAdmin.commands.all')}</Tabs.Trigger>
				</Tabs.List>
			</Tabs.Root>
		{/if}
		<div class="ml-auto flex items-center gap-2">
			{#if listing}
				<span class="text-xs text-muted-foreground tabular-nums">
					{plural('rustAdmin.commands.count', matches.length)}
				</span>
			{/if}
			<Button
				variant="ghost"
				size="icon-sm"
				disabled={loading}
				onclick={load}
				aria-label={t('rustAdmin.refresh')}
				title={t('rustAdmin.refresh')}
			>
				<RefreshCwIcon class={loading ? 'animate-spin' : undefined} />
			</Button>
		</div>
	</div>

	{#if failed}
		<Alert.Root variant="destructive" class="m-5 w-auto">
			<Alert.Title>{t('rustAdmin.failed')}</Alert.Title>
			<Alert.Description>{failed}</Alert.Description>
		</Alert.Root>
	{:else if !listing}
		<div class="space-y-3 p-5">
			{#each [0, 1, 2, 3, 4] as row (row)}
				<Skeleton class="h-10 w-full" />
			{/each}
		</div>
	{:else}
		{#if !running}
			<p class="border-b bg-muted/40 px-5 py-2 text-xs text-muted-foreground">
				{t('rustAdmin.commands.stopped')}
			</p>
		{/if}

		{#if !matches.length}
			<p class="px-5 py-10 text-center text-sm text-muted-foreground">{t('rustAdmin.noMatch')}</p>
		{:else}
			<ul class="divide-y">
				{#each visible as command (command.name)}
					{@const reply = replies[command.name]}
					<li class="px-5 py-3">
						<div class="flex items-start gap-3">
							<div class="min-w-0 flex-1">
								<span class="font-mono text-sm font-medium break-all">{command.name}</span>
								{#if command.help}
									<p
										class="mt-0.5 text-sm text-muted-foreground"
										title={command.generated ? t('rustAdmin.generated') : undefined}
									>
										{command.help}
									</p>
								{/if}
							</div>
							<Button
								variant={open === command.name ? 'secondary' : 'outline'}
								size="sm"
								disabled={!running}
								onclick={() => toggle(command)}
							>
								<TerminalIcon />
								{t('rustAdmin.commands.run')}
							</Button>
						</div>
						{#if open === command.name}
							<form
								class="mt-3 flex gap-2"
								onsubmit={(event) => {
									event.preventDefault();
									send(command.name);
								}}
							>
								<Input
									bind:value={line}
									aria-label={command.name}
									class="font-mono text-xs"
									{@attach (input: HTMLInputElement) => input.focus()}
								/>
								<Button type="submit" size="sm" disabled={sending || !running || !line.trim()}>
									{#if sending}<Spinner class="size-3.5" />{:else}<SendIcon />{/if}
									{t('rustAdmin.commands.send')}
								</Button>
							</form>
							{#if reply}
								<pre
									class={[
										'mt-2 max-h-56 overflow-auto rounded-md bg-muted p-3 font-mono text-xs whitespace-pre-wrap',
										!reply.ok && 'text-destructive'
									]}>{reply.text || t('rustAdmin.commands.noReply')}</pre>
							{/if}
						{/if}
					</li>
				{/each}
			</ul>
			{#if matches.length > visible.length}
				<div class="border-t p-3 text-center">
					<Button variant="ghost" size="sm" onclick={() => (limit += 100)}>
						{t('rustAdmin.showMore', { count: Math.min(100, matches.length - visible.length) })}
					</Button>
				</div>
			{/if}
		{/if}
	{/if}
</section>
