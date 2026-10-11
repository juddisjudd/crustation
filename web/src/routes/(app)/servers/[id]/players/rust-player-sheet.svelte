<script lang="ts">
	import BanIcon from '@lucide/svelte/icons/ban';
	import BootIcon from '@lucide/svelte/icons/door-open';
	import CheckIcon from '@lucide/svelte/icons/check';
	import CopyIcon from '@lucide/svelte/icons/copy';
	import ExternalLinkIcon from '@lucide/svelte/icons/external-link';
	import GiftIcon from '@lucide/svelte/icons/gift';
	import HandHeartIcon from '@lucide/svelte/icons/hand-heart';
	import HeartPulseIcon from '@lucide/svelte/icons/heart-pulse';
	import MapPinIcon from '@lucide/svelte/icons/map-pin';
	import MicOffIcon from '@lucide/svelte/icons/mic-off';
	import MicIcon from '@lucide/svelte/icons/mic';
	import PackageXIcon from '@lucide/svelte/icons/package-x';
	import ShieldIcon from '@lucide/svelte/icons/shield';
	import SkullIcon from '@lucide/svelte/icons/skull';
	import UndoIcon from '@lucide/svelte/icons/undo-2';
	import { Badge } from '$lib/components/ui/badge/index.js';
	import { Button } from '$lib/components/ui/button/index.js';
	import * as Sheet from '$lib/components/ui/sheet/index.js';
	import * as ToggleGroup from '$lib/components/ui/toggle-group/index.js';
	import Meter from '$lib/components/meter.svelte';
	import { steamProfileUrl } from '$lib/api/rust';
	import { dateTime, duration } from '$lib/format';
	import { t } from '$lib/i18n/index.svelte';
	import type { Person, Verb } from './rust-person';

	let {
		person,
		open = $bindable(false),
		running,
		canCommand,
		canTeleport,
		busy,
		act,
		setRole
	}: {
		person: Person | null;
		open?: boolean;
		running: boolean;
		canCommand: boolean;
		/** Somebody else is on to be sent to. */
		canTeleport: boolean;
		busy: boolean;
		act: (verb: Verb, person: Person) => void;
		setRole: (person: Person, role: string) => void;
	} = $props();

	const here = $derived(running && !!person?.on);
	let copied = $state(false);
	const chosen = 'data-[state=on]:bg-foreground data-[state=on]:text-background';

	async function copyId(steamId: string) {
		try {
			await navigator.clipboard.writeText(steamId);
			copied = true;
			setTimeout(() => (copied = false), 1500);
		} catch {
			// clipboard unavailable
		}
	}
</script>

<Sheet.Root bind:open>
	<Sheet.Content class="gap-0 overflow-y-auto sm:max-w-md">
		{#if person}
			{@const who = person}
			{@const steamId = who.steamId ?? ''}
			<Sheet.Header class="border-b pr-12">
				<Sheet.Title class="flex flex-wrap items-center gap-2">
					{who.name}
					{#if who.role}
						<Badge variant="secondary" class="gap-1">
							<ShieldIcon class="size-3" />
							{who.role === 'owner' ? t('players.rust.owner') : t('players.rust.moderator')}
						</Badge>
					{/if}
					{#if who.banned}
						<Badge variant="destructive" class="gap-1">
							<BanIcon class="size-3" />
							{t('players.badge.banned')}
						</Badge>
					{/if}
				</Sheet.Title>
				<Sheet.Description>
					<button
						type="button"
						class="inline-flex cursor-pointer items-center gap-1.5 font-mono text-xs hover:text-foreground"
						title={t('players.rust.copyId')}
						onclick={() => copyId(steamId)}
					>
						{steamId}
						{#if copied}<CheckIcon class="size-3" />{:else}<CopyIcon class="size-3" />{/if}
					</button>
				</Sheet.Description>
			</Sheet.Header>

			<div class="grid gap-6 p-4">
				<dl class="grid grid-cols-2 gap-x-6 gap-y-4 text-sm">
					{#if here}
						<div>
							<dt class="text-muted-foreground">{t('players.rust.status')}</dt>
							<dd class="mt-1">
								<span class="text-success">{t('players.known.here')}</span>
								{#if who.ping !== undefined}
									<span class="ml-1 text-muted-foreground tabular-nums">
										{t('players.rust.ping', { ping: who.ping })}
									</span>
								{/if}
							</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('players.rust.connected')}</dt>
							<dd class="mt-1 tabular-nums">{duration((who.connectedSeconds ?? 0) * 1000)}</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('players.rust.health')}</dt>
							<dd class="mt-1 tabular-nums">
								{Math.round(who.health ?? 0)}
								<Meter
									value={who.health}
									label={t('players.rust.health')}
									tone="neutral"
									class="mt-2"
								/>
							</dd>
						</div>
						<div>
							<dt class="text-muted-foreground">{t('players.rust.address')}</dt>
							<dd class="mt-1 font-mono text-xs break-all">{who.address || '—'}</dd>
						</div>
					{:else}
						<div>
							<dt class="text-muted-foreground">{t('players.known.last')}</dt>
							<dd class="mt-1">{who.last ? dateTime(who.last) : '—'}</dd>
						</div>
					{/if}
					<div>
						<dt class="text-muted-foreground">{t('players.known.first')}</dt>
						<dd class="mt-1">{who.first ? dateTime(who.first) : '—'}</dd>
					</div>
					{#if who.ban}
						<div class="col-span-2">
							<dt class="text-muted-foreground">{t('players.badge.banned')}</dt>
							<dd class="mt-1">
								{who.ban.expires === null
									? t('players.rust.lengths.forever')
									: t('players.rust.until', {
											when: dateTime(new Date(who.ban.expires * 1000).toISOString())
										})}
								{#if who.ban.reason}
									<span class="block text-xs text-muted-foreground">{who.ban.reason}</span>
								{/if}
							</dd>
						</div>
					{/if}
				</dl>

				<section>
					<h3 class="mb-2 text-xs font-medium text-muted-foreground">
						{t('players.rust.moderation')}
					</h3>
					<div class="flex flex-wrap gap-2">
						{#if here}
							<Button variant="outline" size="sm" disabled={busy} onclick={() => act('kick', who)}>
								<BootIcon />
								{t('players.act.kick')}
							</Button>
							<Button variant="outline" size="sm" disabled={busy} onclick={() => act('mute', who)}>
								<MicOffIcon />
								{t('players.rust.mute')}
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={busy}
								onclick={() => act('unmute', who)}
							>
								<MicIcon />
								{t('players.rust.unmute')}
							</Button>
						{/if}
						{#if who.banned}
							<Button variant="outline" size="sm" disabled={busy} onclick={() => act('unban', who)}>
								<UndoIcon />
								{t('players.act.pardon')}
							</Button>
						{:else}
							<Button
								variant="outline"
								class="text-destructive hover:text-destructive"
								size="sm"
								disabled={busy}
								onclick={() => act('ban', who)}
							>
								<BanIcon />
								{t('players.act.ban')}
							</Button>
						{/if}
					</div>
				</section>

				<section>
					<h3 class="mb-2 text-xs font-medium text-muted-foreground">
						{t('players.rust.staffRole')}
					</h3>
					<ToggleGroup.Root
						type="single"
						variant="outline"
						size="sm"
						disabled={busy}
						value={who.role ?? 'none'}
						onValueChange={(next) => next && setRole(who, next)}
					>
						<ToggleGroup.Item value="none" class={chosen}
							>{t('players.rust.noRole')}</ToggleGroup.Item
						>
						<ToggleGroup.Item value="moderator" class={chosen}
							>{t('players.rust.moderator')}</ToggleGroup.Item
						>
						<ToggleGroup.Item value="owner" class={chosen}
							>{t('players.rust.owner')}</ToggleGroup.Item
						>
					</ToggleGroup.Root>
				</section>

				{#if canCommand && here}
					<section>
						<h3 class="mb-2 text-xs font-medium text-muted-foreground">
							{t('players.rust.inGame')}
						</h3>
						<div class="flex flex-wrap gap-2">
							<Button variant="outline" size="sm" disabled={busy} onclick={() => act('give', who)}>
								<GiftIcon />
								{t('players.act.give')}
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={busy || !canTeleport}
								onclick={() => act('teleport', who)}
							>
								<MapPinIcon />
								{t('players.act.teleport')}
							</Button>
							<Button variant="outline" size="sm" disabled={busy} onclick={() => act('heal', who)}>
								<HeartPulseIcon />
								{t('players.rust.heal')}
							</Button>
							<Button
								variant="outline"
								size="sm"
								disabled={busy}
								onclick={() => act('revive', who)}
							>
								<HandHeartIcon />
								{t('players.rust.revive')}
							</Button>
							<Button
								variant="outline"
								class="text-destructive hover:text-destructive"
								size="sm"
								disabled={busy}
								onclick={() => act('kill', who)}
							>
								<SkullIcon />
								{t('players.rust.kill')}
							</Button>
							<Button
								variant="outline"
								class="text-destructive hover:text-destructive"
								size="sm"
								disabled={busy}
								onclick={() => act('clear-inventory', who)}
							>
								<PackageXIcon />
								{t('players.rust.clearInventory')}
							</Button>
						</div>
					</section>
				{:else if !here}
					<p class="text-xs text-muted-foreground">{t('players.rust.away', { name: who.name })}</p>
				{/if}
			</div>

			<Sheet.Footer class="border-t">
				<Button variant="outline" href={steamProfileUrl(steamId)} target="_blank" rel="noreferrer">
					<ExternalLinkIcon />
					{t('players.rust.profile')}
				</Button>
			</Sheet.Footer>
		{/if}
	</Sheet.Content>
</Sheet.Root>
