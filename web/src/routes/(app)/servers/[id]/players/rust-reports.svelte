<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';
	import { toast } from 'svelte-sonner';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import { errorMessage } from '$lib/api/servers';
	import {
		deleteReport,
		reportImageUrl,
		rustReports,
		setReportInbox,
		type RustReport
	} from '$lib/api/rust';
	import type { Server } from '$lib/api/types';
	import { socket } from '$lib/realtime/socket.svelte';
	import { dateTime } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { server }: { server: Server } = $props();

	const canConfigure = $derived(server.permissions.includes('CONFIG'));

	let enabled = $state(false);
	let reports = $state.raw<RustReport[]>([]);
	let busy = $state(false);

	async function load() {
		try {
			const found = await rustReports(server.id);
			enabled = found.enabled;
			reports = found.reports;
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		}
	}

	$effect(() => {
		void server.id;
		load();
	});

	$effect(() => socket.on(`server:${server.id}`, 'report', () => load()));

	async function act(work: () => Promise<unknown>) {
		busy = true;
		try {
			await work();
			await load();
		} catch (err) {
			toast.error(t('players.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	const kind = (value: number) => t(`players.rust.kinds.k${value}` as MessageKey);
</script>

<div class="mt-4 space-y-4">
	<div class="flex items-start gap-3">
		<Switch
			id="report-inbox"
			checked={enabled}
			disabled={busy || !canConfigure}
			onCheckedChange={(on) => act(() => setReportInbox(server.id, on))}
		/>
		<div class="space-y-0.5">
			<Label for="report-inbox">{t('players.rust.inbox')}</Label>
			<p class="text-xs text-muted-foreground">{t('players.rust.inboxHint')}</p>
		</div>
	</div>

	{#if reports.length === 0}
		<p class="text-sm text-muted-foreground">{t('players.rust.noReports')}</p>
	{:else}
		<ul class="space-y-3">
			{#each reports as report (report.id)}
				<li class="rounded-lg border p-3">
					<div class="flex items-start justify-between gap-3">
						<div class="min-w-0 space-y-1">
							<div class="flex flex-wrap items-center gap-2">
								<Badge variant="secondary">{kind(report.kind)}</Badge>
								<span class="font-medium">{report.subject}</span>
							</div>
							<p class="text-xs text-muted-foreground">
								{dateTime(report.received_at)} ·
								{t('players.rust.from', { name: report.reporter_name ?? report.reporter_id })}
								{#if report.target_name || report.target_id}
									·
									{t('players.rust.about', {
										name: report.target_name ?? report.target_id ?? ''
									})}
								{/if}
							</p>
						</div>
						<Button
							variant="ghost"
							size="icon-sm"
							aria-label={t('players.remove', { name: report.subject })}
							disabled={busy}
							onclick={() => act(() => deleteReport(server.id, report.id))}
						>
							<XIcon />
						</Button>
					</div>
					{#if report.message}
						<p class="mt-2 text-sm wrap-break-word whitespace-pre-wrap">{report.message}</p>
					{/if}
					{#if report.has_image}
						<a
							href={reportImageUrl(server.id, report.id)}
							target="_blank"
							rel="noreferrer"
							class="mt-2 block w-fit"
						>
							<img
								src={reportImageUrl(server.id, report.id)}
								alt={t('players.rust.screenshot')}
								class="max-h-40 rounded border"
								loading="lazy"
							/>
						</a>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</div>
