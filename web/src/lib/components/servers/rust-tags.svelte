<script lang="ts">
	import XIcon from '@lucide/svelte/icons/x';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	interface Props {
		id?: string;
		value: string;
		onchange: (value: string) => void;
	}

	let { id, value, onchange }: Props = $props();

	/** The browser's groups; an exclusive one takes a single tag, like a radio button. */
	const GROUPS = [
		{ key: 'wipe', exclusive: true, tags: ['monthly', 'biweekly', 'weekly'] },
		{ key: 'difficulty', exclusive: true, tags: ['vanilla', 'hardcore', 'softcore'] },
		{
			key: 'gameplay',
			exclusive: false,
			tags: [
				'pve',
				'roleplay',
				'creative',
				'minigame',
				'training',
				'battlefield',
				'broyale',
				'builds',
				'tut'
			]
		},
		{ key: 'modding', exclusive: false, tags: ['premium', 'modded', 'oxide', 'carbon'] },
		{ key: 'region', exclusive: true, tags: ['na', 'sa', 'eu', 'wa', 'ea', 'oc', 'af'] }
	] as const;
	const KNOWN = new Set<string>(GROUPS.flatMap((group) => group.tags));

	const chosen = $derived(
		value
			.split(',')
			.map((tag) => tag.trim().toLowerCase())
			.filter(Boolean)
	);
	const unknown = $derived(chosen.filter((tag) => !KNOWN.has(tag)));

	function toggle(group: (typeof GROUPS)[number], tag: string) {
		let next = chosen.includes(tag)
			? chosen.filter((one) => one !== tag)
			: [
					...chosen.filter(
						(one) => !group.exclusive || !(group.tags as readonly string[]).includes(one)
					),
					tag
				];
		next = [...new Set(next)];
		onchange(next.join(','));
	}

	const label = (group: string, tag: string) =>
		group === 'region' ? tag.toUpperCase() : t(`settings.tags.names.${tag}` as MessageKey);
</script>

<div {id} class="space-y-2">
	{#each GROUPS as group (group.key)}
		<div class="flex flex-wrap items-center gap-1.5">
			<span class="w-20 shrink-0 text-xs text-muted-foreground">
				{t(`settings.tags.groups.${group.key}`)}
			</span>
			{#each group.tags as tag (tag)}
				{@const on = chosen.includes(tag)}
				<button
					type="button"
					aria-pressed={on}
					onclick={() => toggle(group, tag)}
					class={[
						'rounded-full border px-2.5 py-0.5 text-xs transition-colors',
						on
							? 'border-primary bg-primary text-primary-foreground'
							: 'text-muted-foreground hover:border-foreground/30 hover:text-foreground'
					]}
				>
					{label(group.key, tag)}
				</button>
			{/each}
		</div>
	{/each}
	{#if unknown.length}
		<div class="flex flex-wrap items-center gap-1.5">
			<span class="w-20 shrink-0 text-xs text-muted-foreground">{t('settings.tags.other')}</span>
			{#each unknown as tag (tag)}
				<button
					type="button"
					onclick={() => onchange(chosen.filter((one) => one !== tag).join(','))}
					class="inline-flex items-center gap-1 rounded-full border px-2.5 py-0.5 text-xs"
					aria-label={t('players.remove', { name: tag })}
				>
					{tag}
					<XIcon class="size-3" />
				</button>
			{/each}
		</div>
	{/if}
	{#if chosen.length > 4}
		<p class="text-xs text-warning">{t('settings.tags.tooMany', { count: chosen.length })}</p>
	{/if}
</div>
