<script lang="ts">
	import * as Card from '$lib/components/ui/card/index.js';
	import { rustHealth, type RustHealth, type RustPlus } from '$lib/api/rust';
	import { bytes } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';

	let { serverId }: { serverId: string } = $props();

	let health = $state.raw<RustHealth | null>(null);
	let rustPlus = $state.raw<RustPlus | null>(null);

	const port = $derived(
		(rustPlus?.connects_to ?? rustPlus?.listening ?? '').split(':').at(-1) ?? ''
	);

	$effect(() => {
		const id = serverId;
		const look = () =>
			rustHealth(id)
				.then((found) => {
					health = found.health;
					rustPlus = found.rust_plus;
				})
				.catch(() => (health = null));
		look();
		const timer = setInterval(look, 10_000);
		return () => clearInterval(timer);
	});
</script>

{#if health}
	<Card.Root>
		<Card.Header>
			<Card.Title>{t('server.health.title')}</Card.Title>
		</Card.Header>
		<Card.Content>
			<dl class="grid grid-cols-2 gap-3 text-sm">
				<div>
					<dt class="text-muted-foreground">{t('server.health.framerate')}</dt>
					<dd class="mt-1 tabular-nums">
						{t('server.health.fps', { fps: Math.round(health.framerate) })}
					</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.health.entities')}</dt>
					<dd class="mt-1 tabular-nums">{health.entities.toLocaleString()}</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.health.queue')}</dt>
					<dd class="mt-1 tabular-nums">
						{t('server.health.queueValue', { queued: health.queued, joining: health.joining })}
					</dd>
				</div>
				<div>
					<dt class="text-muted-foreground">{t('server.health.network')}</dt>
					<dd class="mt-1 tabular-nums">
						{t('server.health.networkValue', {
							in: bytes(health.network_in),
							out: bytes(health.network_out)
						})}
					</dd>
				</div>
				{#if rustPlus}
					<div class="col-span-2">
						<dt class="text-muted-foreground">{t('server.health.rustPlus')}</dt>
						<dd class="mt-1 space-y-0.5">
							<span class={rustPlus.state === 'failed' ? 'text-destructive' : undefined}>
								{t(`server.health.rustPlusStates.${rustPlus.state}`)}
							</span>
							{#if rustPlus.state === 'failed'}
								{#if rustPlus.detail}
									<span class="block text-xs text-muted-foreground">{rustPlus.detail}</span>
								{/if}
								<span class="block text-xs text-muted-foreground">
									{t('server.health.rustPlusFix', { port })}
								</span>
							{:else if rustPlus.connects_to}
								<span class="block text-xs text-muted-foreground">
									{t('server.health.rustPlusWhere', { address: rustPlus.connects_to })}
								</span>
							{/if}
						</dd>
					</div>
				{/if}
			</dl>
		</Card.Content>
	</Card.Root>
{/if}
