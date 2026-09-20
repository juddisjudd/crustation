<script lang="ts">
	import EraserIcon from '@lucide/svelte/icons/eraser';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Spinner } from '$lib/components/ui/spinner/index.js';
	import { Textarea } from '$lib/components/ui/textarea/index.js';
	import * as Tooltip from '$lib/components/ui/tooltip/index.js';
	import { errorMessage } from '$lib/api/servers';
	import { readProperties, writeProperties } from '$lib/api/properties';
	import { pieces } from '$lib/console';
	import { t } from '$lib/i18n/index.svelte';

	interface Props {
		serverId: string;
		/** Java keeps it in `motd`, Bedrock in `server-name`. */
		edition: 'java' | 'bedrock';
		canEdit: boolean;
	}

	let { serverId, edition, canEdit }: Props = $props();

	const key = $derived(edition === 'bedrock' ? 'server-name' : 'motd');
	/** Java shows two lines in the list; Bedrock shows one. */
	const lines = $derived(edition === 'bedrock' ? 1 : 2);

	const COLOURS = [
		['0', '#000000'],
		['1', '#0000aa'],
		['2', '#00aa00'],
		['3', '#00aaaa'],
		['4', '#aa0000'],
		['5', '#aa00aa'],
		['6', '#ffaa00'],
		['7', '#aaaaaa'],
		['8', '#555555'],
		['9', '#5555ff'],
		['a', '#55ff55'],
		['b', '#55ffff'],
		['c', '#ff5555'],
		['d', '#ff55ff'],
		['e', '#ffff55'],
		['f', '#ffffff']
	] as const;

	const STYLES = [
		['l', 'Bold'],
		['o', 'Italic'],
		['n', 'Underline'],
		['m', 'Strikethrough'],
		['r', 'Reset']
	] as const;

	let text = $state('');
	let saved = $state('');
	let loading = $state(true);
	let saving = $state(false);
	let box = $state<HTMLTextAreaElement | null>(null);

	const dirty = $derived(text !== saved);
	const overlong = $derived(text.split('\n').some((line) => line.replace(/§./g, '').length > 59));
	const tooTall = $derived(text.split('\n').length > lines);

	$effect(() => {
		const id = serverId;
		const wanted = key;
		loading = true;
		readProperties(id)
			.then((file) => {
				if (serverId !== id) return;
				const found =
					file.settings.find((one) => one.key === wanted)?.value ??
					file.other.find((one) => one.key === wanted)?.value ??
					'';
				saved = found;
				text = found;
			})
			.catch(() => {})
			.finally(() => {
				if (serverId === id) loading = false;
			});
	});

	function insert(code: string) {
		const at = box?.selectionStart ?? text.length;
		const to = box?.selectionEnd ?? at;
		text = `${text.slice(0, at)}§${code}${text.slice(to)}`;
		queueMicrotask(() => {
			box?.focus();
			box?.setSelectionRange(at + 2, at + 2);
		});
	}

	async function save() {
		saving = true;
		try {
			const result = await writeProperties(serverId, { [key]: text });
			saved = text;
			toast.success(t('server.motd.saved'), {
				description: result.restart_required ? t('server.motd.onRestart') : undefined
			});
		} catch (err) {
			toast.error(t('server.motd.failed'), { description: errorMessage(err) });
		} finally {
			saving = false;
		}
	}
</script>

<div class="space-y-3">
	<div
		class="rounded-md border bg-[#0a0a0a] px-3 py-2 font-mono text-[13px] leading-5 text-[#aaaaaa]"
		aria-label={t('server.motd.preview')}
	>
		{#each text.split('\n').slice(0, lines) as line, index (index)}
			<div class="min-h-5 break-all">
				{#each pieces(line, null) as piece, at (at)}
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
			</div>
		{:else}
			<div class="text-white/25">{t('server.motd.empty')}</div>
		{/each}
	</div>

	{#if canEdit}
		<Textarea
			bind:ref={box}
			bind:value={text}
			rows={2}
			disabled={loading}
			spellcheck={false}
			class="font-mono text-sm"
			aria-label={t('server.overview.motd')}
		/>

		<div class="grid w-max grid-cols-8 gap-1">
			{#each COLOURS as [code, colour] (code)}
				<Tooltip.Root>
					<Tooltip.Trigger>
						{#snippet child({ props })}
							<button
								{...props}
								type="button"
								class="size-5 rounded-sm border border-border/60 transition-transform hover:scale-110"
								style="background: {colour}"
								aria-label={`§${code}`}
								onclick={() => insert(code)}
							></button>
						{/snippet}
					</Tooltip.Trigger>
					<Tooltip.Content>§{code}</Tooltip.Content>
				</Tooltip.Root>
			{/each}
		</div>

		<div class="flex flex-wrap items-center gap-1">
			{#each STYLES as [code, name] (code)}
				<Button
					variant="outline"
					size="sm"
					class="h-5 px-1.5 text-[10px]"
					onclick={() => insert(code)}
				>
					{name}
				</Button>
			{/each}
			<Button
				variant="ghost"
				size="icon-sm"
				class="size-5"
				aria-label={t('server.motd.strip')}
				onclick={() => (text = text.replace(/§./g, ''))}
			>
				<EraserIcon class="size-3" />
			</Button>
		</div>

		{#if tooTall}
			<p class="text-xs text-warning">{t('server.motd.tooTall', { lines })}</p>
		{:else if overlong}
			<p class="text-xs text-warning">{t('server.motd.overlong')}</p>
		{/if}

		<div class="flex items-center gap-2">
			<Button size="sm" disabled={!dirty || saving || loading} onclick={save}>
				{#if saving}<Spinner />{/if}
				{t('common.actions.save')}
			</Button>
			{#if dirty}
				<Button size="sm" variant="ghost" onclick={() => (text = saved)}>
					{t('common.actions.reset')}
				</Button>
			{/if}
		</div>
	{/if}
</div>
