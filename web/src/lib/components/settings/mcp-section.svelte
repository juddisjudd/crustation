<script lang="ts">
	import CheckIcon from '@lucide/svelte/icons/check';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import KeyIcon from '@lucide/svelte/icons/key';
	import { toast } from 'svelte-sonner';
	import { resolve } from '$app/paths';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import SettingsCard from '$lib/components/settings-card.svelte';
	import { errorMessage } from '$lib/api/servers';
	import { mcpStatus, setMcp, type McpStatus } from '$lib/api/mcp';
	import { t } from '$lib/i18n/index.svelte';

	let status = $state.raw<McpStatus | null>(null);
	let busy = $state(false);
	let copied = $state('');

	// The panel knows its own address only when one was configured; otherwise the
	// one the browser is already using is the best guess going.
	const origin = $derived(
		status?.public_url?.replace(/\/$/, '') ??
			(typeof location === 'undefined' ? '' : location.origin)
	);
	const endpoint = $derived(`${origin}/mcp`);
	const command = $derived(
		`claude mcp add --transport http crustation ${endpoint} \\\n  --header "Authorization: Bearer <api key>"`
	);

	async function look() {
		try {
			status = await mcpStatus();
		} catch (err) {
			toast.error(t('mcp.failed'), { description: errorMessage(err) });
		}
	}

	$effect(() => {
		look();
	});

	async function toggle(on: boolean) {
		busy = true;
		try {
			const done = await setMcp(on);
			status = status && { ...status, enabled: done.enabled };
			toast.success(done.enabled ? t('mcp.turnedOn') : t('mcp.turnedOff'));
		} catch (err) {
			toast.error(t('mcp.failed'), { description: errorMessage(err) });
			await look();
		} finally {
			busy = false;
		}
	}

	async function copy(what: string, text: string) {
		try {
			await navigator.clipboard.writeText(text);
			copied = what;
			setTimeout(() => (copied = ''), 1500);
		} catch {
			// clipboard unavailable
		}
	}
</script>

{#if !status}
	<Skeleton class="h-40 rounded-lg" />
{:else}
	{@const live = status}
	<div class="space-y-4">
		<SettingsCard title={t('mcp.title')} description={t('mcp.hint')}>
			<div class="flex flex-wrap items-center justify-between gap-4">
				<div class="min-w-0">
					<Label for="mcp-on" class="text-sm font-medium">{t('mcp.toggle')}</Label>
					<p class="mt-1 text-sm text-muted-foreground">{t('mcp.toggleHint')}</p>
				</div>
				<div class="flex items-center gap-3">
					{#if live.enabled}
						<Badge class="bg-success text-background">{t('mcp.enabled')}</Badge>
					{:else}
						<Badge variant="outline">{t('mcp.disabled')}</Badge>
					{/if}
					<Switch
						id="mcp-on"
						checked={live.enabled}
						disabled={busy}
						onCheckedChange={(on) => toggle(on)}
					/>
				</div>
			</div>

			{#snippet hint()}
				{t('mcp.configNote', {
					value: live.config_default ? t('mcp.configOn') : t('mcp.configOff')
				})}
			{/snippet}
		</SettingsCard>

		<SettingsCard title={t('mcp.connect')} description={t('mcp.connectHint')}>
			<div class="space-y-2">
				<Label class="text-xs text-muted-foreground">{t('mcp.endpoint')}</Label>
				<div class="flex items-center gap-2">
					<code
						class="min-w-0 flex-1 truncate rounded-md border bg-muted/40 px-3 py-2 font-mono text-xs"
					>
						{endpoint}
					</code>
					<Button
						variant="outline"
						size="icon-sm"
						aria-label={t('mcp.copy')}
						onclick={() => copy('endpoint', endpoint)}
					>
						{#if copied === 'endpoint'}<CheckIcon />{:else}<CopyIcon />{/if}
					</Button>
				</div>
			</div>

			<div class="relative">
				<pre
					class="overflow-x-auto rounded-md border bg-muted/40 p-3 pr-12 font-mono text-xs leading-5">{command}</pre>
				<Button
					variant="outline"
					size="icon-sm"
					class="absolute top-2 right-2"
					aria-label={t('mcp.copy')}
					onclick={() => copy('command', command)}
				>
					{#if copied === 'command'}<CheckIcon />{:else}<CopyIcon />{/if}
				</Button>
			</div>

			<Alert.Root>
				<KeyIcon />
				<Alert.Title>{t('mcp.keyTitle')}</Alert.Title>
				<Alert.Description class="space-y-2">
					<p>{t('mcp.keyBody')}</p>
					<p>{t('mcp.keyScope')}</p>
				</Alert.Description>
				<Alert.Action>
					<Button variant="outline" size="sm" href={resolve('/users')}>
						{t('mcp.keyAction')}
					</Button>
				</Alert.Action>
			</Alert.Root>
		</SettingsCard>

		<SettingsCard title={t('mcp.tools')} description={t('mcp.toolsHint')}>
			<div class="overflow-hidden rounded-lg border">
				<Table.Root>
					<Table.Header>
						<Table.Row class="bg-muted/50 hover:bg-muted/50">
							<Table.Head>{t('mcp.columnTool')}</Table.Head>
							<Table.Head class="w-32">{t('mcp.columnNeeds')}</Table.Head>
						</Table.Row>
					</Table.Header>
					<Table.Body>
						{#each live.tools as tool (tool.name)}
							<Table.Row>
								<Table.Cell>
									<div class="font-mono text-xs font-medium">{tool.name}</div>
									{#if tool.description}
										<div class="mt-1 text-xs text-muted-foreground">{tool.description}</div>
									{/if}
								</Table.Cell>
								<Table.Cell class="align-top">
									{#if tool.permission}
										<Badge variant="secondary" class="font-mono text-[11px]">
											{tool.permission}
										</Badge>
									{:else}
										<span class="text-xs text-muted-foreground">{t('mcp.needsNothing')}</span>
									{/if}
								</Table.Cell>
							</Table.Row>
						{/each}
					</Table.Body>
				</Table.Root>
			</div>

			{#snippet hint()}
				<span class="font-mono text-[11px]">{live.resources.join('   ')}</span>
			{/snippet}
		</SettingsCard>
	</div>
{/if}
