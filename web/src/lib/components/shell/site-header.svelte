<script lang="ts">
	import { page } from '$app/state';
	import * as Breadcrumb from '$lib/components/ui/breadcrumb/index.js';
	import * as Sidebar from '$lib/components/ui/sidebar/index.js';
	import { Separator } from '$lib/components/ui/separator/index.js';
	import { socket } from '$lib/realtime/socket.svelte';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	const crumbs = $derived(page.data.crumbs ?? []);
	const label = (crumb: { label?: string; labelKey?: MessageKey }) =>
		crumb.labelKey ? t(crumb.labelKey) : (crumb.label ?? '');
	const offline = $derived(socket.status === 'closed');
</script>

<header
	class="sticky top-0 z-10 flex h-12 shrink-0 items-center gap-2 border-b bg-background/80 px-3 backdrop-blur supports-backdrop-filter:bg-background/60 md:rounded-t-xl"
>
	<Sidebar.Trigger class="-ml-1 text-muted-foreground" />
	<Separator orientation="vertical" class="mr-1 data-[orientation=vertical]:h-4" />
	<Breadcrumb.Root>
		<Breadcrumb.List>
			{#each crumbs as crumb, i (i)}
				{#if i > 0}<Breadcrumb.Separator />{/if}
				<Breadcrumb.Item class={i < crumbs.length - 1 ? 'hidden md:inline-flex' : ''}>
					{#if crumb.href && i < crumbs.length - 1}
						<Breadcrumb.Link href={crumb.href}>{label(crumb)}</Breadcrumb.Link>
					{:else}
						<Breadcrumb.Page class="max-w-[40ch] truncate">{label(crumb)}</Breadcrumb.Page>
					{/if}
				</Breadcrumb.Item>
			{/each}
		</Breadcrumb.List>
	</Breadcrumb.Root>
	{#if offline}
		<span
			class="ml-auto inline-flex items-center gap-1.5 rounded-full border px-2 py-0.5 text-xs text-muted-foreground"
			role="status"
		>
			<span class="size-1.5 animate-pulse rounded-full bg-warning"></span>
			{t('nav.reconnecting')}
		</span>
	{/if}
</header>
