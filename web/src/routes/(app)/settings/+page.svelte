<script lang="ts">
	import PageHeader from '$lib/components/page-header.svelte';
	import McpSection from '$lib/components/settings/mcp-section.svelte';
	import { t } from '$lib/i18n/index.svelte';
</script>

<svelte:head><title>{t('panel.title')} · Crustation</title></svelte:head>

<div class="mx-auto w-full max-w-4xl space-y-8 px-4 py-8 md:px-8">
	<PageHeader title={t('panel.title')} description={t('panel.hint')} />

	<section class="space-y-4">
		<h2 class="text-sm font-medium text-muted-foreground">{t('panel.integrations')}</h2>
		<McpSection />
	</section>
</div>
