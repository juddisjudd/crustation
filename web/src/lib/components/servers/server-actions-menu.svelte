<script lang="ts">
	import EllipsisIcon from '@lucide/svelte/icons/ellipsis';
	import PlayIcon from '@lucide/svelte/icons/play';
	import SquareIcon from '@lucide/svelte/icons/square';
	import RotateCcwIcon from '@lucide/svelte/icons/rotate-ccw';
	import SkullIcon from '@lucide/svelte/icons/skull';
	import SquareTerminalIcon from '@lucide/svelte/icons/square-terminal';
	import { resolve } from '$app/paths';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { powerAction } from '$lib/api/servers';
	import { servers } from '$lib/servers.svelte';
	import { t } from '$lib/i18n/index.svelte';
	import type { Server } from '$lib/api/types';

	let { server, align = 'end' }: { server: Server; align?: 'start' | 'end' } = $props();

	const running = $derived(servers.isRunning(server.id));
	const canCommand = $derived(server.permissions.includes('COMMANDS'));
	const canConsole = $derived(server.permissions.includes('CONSOLE'));

	async function kill() {
		const ok = await confirm({
			title: t('server.killTitle', { name: server.name }),
			description: t('server.killBody'),
			confirmLabel: t('server.killConfirm'),
			destructive: true
		});
		if (ok) powerAction(server.id, 'kill', server.name);
	}
</script>

<DropdownMenu.Root>
	<DropdownMenu.Trigger>
		{#snippet child({ props })}
			<Button variant="ghost" size="icon-sm" aria-label={t('server.actions')} {...props}>
				<EllipsisIcon />
			</Button>
		{/snippet}
	</DropdownMenu.Trigger>
	<DropdownMenu.Content {align} class="w-48">
		{#if canConsole}
			<DropdownMenu.Item>
				{#snippet child({ props })}
					<a href={resolve(`/servers/${server.id}/console`)} {...props}>
						<SquareTerminalIcon />
						{t('server.openConsole')}
					</a>
				{/snippet}
			</DropdownMenu.Item>
		{/if}
		{#if canCommand}
			{#if canConsole}<DropdownMenu.Separator />{/if}
			{#if running}
				<DropdownMenu.Item onSelect={() => powerAction(server.id, 'stop', server.name)}>
					<SquareIcon />
					{t('common.server.action.stop')}
				</DropdownMenu.Item>
				<DropdownMenu.Item onSelect={() => powerAction(server.id, 'restart', server.name)}>
					<RotateCcwIcon />
					{t('common.server.action.restart')}
				</DropdownMenu.Item>
			{:else}
				<DropdownMenu.Item onSelect={() => powerAction(server.id, 'start', server.name)}>
					<PlayIcon />
					{t('common.server.action.start')}
				</DropdownMenu.Item>
			{/if}
			<DropdownMenu.Separator />
			<DropdownMenu.Item variant="destructive" onSelect={kill}>
				<SkullIcon />
				{t('common.server.action.kill')}
			</DropdownMenu.Item>
		{/if}
		{#if !canConsole && !canCommand}
			<DropdownMenu.Label class="font-normal text-muted-foreground">
				{t('server.noActions')}
			</DropdownMenu.Label>
		{/if}
	</DropdownMenu.Content>
</DropdownMenu.Root>
