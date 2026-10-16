<script lang="ts">
	import MoreIcon from '@lucide/svelte/icons/ellipsis';
	import PencilIcon from '@lucide/svelte/icons/pencil';
	import PlayIcon from '@lucide/svelte/icons/play';
	import PlusIcon from '@lucide/svelte/icons/plus';
	import Trash2Icon from '@lucide/svelte/icons/trash-2';
	import { toast } from 'svelte-sonner';
	import { Button } from '$lib/components/ui/button/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Input } from '$lib/components/ui/input/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { Skeleton } from '$lib/components/ui/skeleton/index.js';
	import { Switch } from '$lib/components/ui/switch/index.js';
	import * as Alert from '$lib/components/ui/alert/index.js';
	import * as Dialog from '$lib/components/ui/dialog/index.js';
	import * as DropdownMenu from '$lib/components/ui/dropdown-menu/index.js';
	import * as Select from '$lib/components/ui/select/index.js';
	import * as Table from '$lib/components/ui/table/index.js';
	import { confirm } from '$lib/components/confirm/confirm.svelte';
	import { errorMessage } from '$lib/api/servers';
	import {
		createSchedule,
		deleteSchedule,
		listSchedules,
		runSchedule,
		updateSchedule,
		type Action,
		type Schedule,
		type ScheduleInput,
		type Trigger,
		type WipeKind
	} from '$lib/api/schedules';
	import { dateTime, isSteam, takesCommands } from '$lib/format';
	import { t, type MessageKey } from '$lib/i18n/index.svelte';

	let { data } = $props();

	const server = $derived(data.server);
	const rust = $derived(server.kind === 'rust');
	const steam = $derived(isSteam(server.kind));
	// Enshrouded and Dragonwilds read no commands, so nothing can be run or said in chat.
	const talks = $derived(takesCommands(server.kind));

	type Unit = 'minutes' | 'hours' | 'days';
	const UNIT_SECONDS: Record<Unit, number> = { minutes: 60, hours: 3600, days: 86_400 };
	const WARNINGS = [0, 60, 300, 900];

	let schedules = $state.raw<Schedule[] | null>(null);
	let loadFailed = $state(false);
	let busy = $state(false);

	let open = $state(false);
	let editing = $state<Schedule | null>(null);
	let name = $state('');
	let enabled = $state(true);
	let oneTime = $state(false);
	let triggerType = $state<Trigger['type']>('cron');
	let every = $state(6);
	let unit = $state<Unit>('hours');
	let expression = $state('0 4 * * *');
	let after = $state('');
	let delayMinutes = $state(5);
	let actionType = $state<Action['type']>('restart');
	let warn = $state('300');
	let command = $state('');
	let wipeKind = $state<WipeKind>('map');
	let newSeed = $state(true);
	let onlyIfNewer = $state(true);

	const actionTypes = $derived<Action['type'][]>(
		rust
			? ['start', 'stop', 'restart', 'command', 'wipe', 'update']
			: steam
				? ['start', 'stop', 'restart', 'update']
				: ['start', 'stop', 'restart', 'command']
	);
	const warns = $derived(talks && ['stop', 'restart', 'wipe', 'update'].includes(actionType));

	async function load() {
		loadFailed = false;
		try {
			schedules = await listSchedules(server.id);
		} catch {
			loadFailed = true;
		}
	}

	$effect(() => {
		void server.id;
		load();
		const timer = setInterval(load, 15_000);
		return () => clearInterval(timer);
	});

	function reset() {
		name = '';
		enabled = true;
		oneTime = false;
		triggerType = 'cron';
		every = 6;
		unit = 'hours';
		expression = '0 4 * * *';
		after = '';
		delayMinutes = 5;
		actionType = 'restart';
		warn = '300';
		command = '';
		wipeKind = 'map';
		newSeed = true;
		onlyIfNewer = true;
	}

	function openNew() {
		editing = null;
		reset();
		open = true;
	}

	function openEdit(schedule: Schedule) {
		reset();
		editing = schedule;
		name = schedule.name;
		enabled = schedule.enabled;
		oneTime = schedule.one_time;
		const trigger = schedule.trigger;
		triggerType = trigger.type;
		if (trigger.type === 'interval') {
			unit =
				trigger.every_seconds % 86_400 === 0
					? 'days'
					: trigger.every_seconds % 3600 === 0
						? 'hours'
						: 'minutes';
			every = trigger.every_seconds / UNIT_SECONDS[unit];
		} else if (trigger.type === 'cron') {
			expression = trigger.expression;
		} else {
			after = trigger.schedule_id;
			delayMinutes = Math.round(trigger.delay_seconds / 60);
		}
		const action = schedule.action;
		actionType = action.type;
		if ('warn_seconds' in action) warn = String(action.warn_seconds);
		if (action.type === 'command') command = action.command;
		if (action.type === 'wipe') {
			wipeKind = action.kind;
			newSeed = action.new_seed;
		}
		if (action.type === 'update') onlyIfNewer = action.only_if_newer;
		open = true;
	}

	function preset(kind: 'nightly' | 'weekly' | 'monthly' | 'updates') {
		reset();
		name = t(`schedules.presets.${kind}`);
		if (kind === 'nightly') {
			expression = '0 4 * * *';
			actionType = 'restart';
		} else if (kind === 'weekly') {
			expression = '0 19 * * 4';
			actionType = 'wipe';
			wipeKind = 'map';
		} else if (kind === 'monthly') {
			expression = '0 20 1-7 * 4';
			actionType = 'wipe';
			wipeKind = 'full';
		} else {
			triggerType = 'interval';
			every = 30;
			unit = 'minutes';
			actionType = 'update';
			onlyIfNewer = true;
		}
	}

	function input(): ScheduleInput {
		const trigger: Trigger =
			triggerType === 'interval'
				? { type: 'interval', every_seconds: Math.round(every * UNIT_SECONDS[unit]) }
				: triggerType === 'cron'
					? { type: 'cron', expression: expression.trim() }
					: { type: 'after', schedule_id: after, delay_seconds: Math.round(delayMinutes * 60) };
		const warn_seconds = talks ? Number(warn) : 0;
		const action: Action =
			actionType === 'start'
				? { type: 'start' }
				: actionType === 'stop'
					? { type: 'stop', warn_seconds }
					: actionType === 'restart'
						? { type: 'restart', warn_seconds }
						: actionType === 'command'
							? { type: 'command', command: command.trim() }
							: actionType === 'wipe'
								? { type: 'wipe', kind: wipeKind, new_seed: newSeed, warn_seconds }
								: { type: 'update', warn_seconds, only_if_newer: onlyIfNewer };
		return { name: name.trim(), enabled, one_time: oneTime, trigger, action };
	}

	async function save() {
		busy = true;
		try {
			const sent = input();
			if (editing) await updateSchedule(server.id, editing.id, sent);
			else await createSchedule(server.id, sent);
			toast.success(t('schedules.saved', { name: sent.name }));
			open = false;
			await load();
		} catch (err) {
			toast.error(t('schedules.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function act(work: () => Promise<unknown>, done: string) {
		busy = true;
		try {
			await work();
			toast.success(done);
			await load();
		} catch (err) {
			toast.error(t('schedules.failed'), { description: errorMessage(err) });
		} finally {
			busy = false;
		}
	}

	async function remove(schedule: Schedule) {
		const confirmed = await confirm({
			title: t('schedules.confirmDelete', { name: schedule.name }),
			confirmLabel: t('common.actions.delete'),
			destructive: true
		});
		if (confirmed) {
			await act(
				() => deleteSchedule(server.id, schedule.id),
				t('schedules.deleted', { name: schedule.name })
			);
		}
	}

	function describeTrigger(trigger: Trigger) {
		if (trigger.type === 'cron') return trigger.expression;
		if (trigger.type === 'after') {
			const parent = schedules?.find((one) => one.id === trigger.schedule_id);
			return t('schedules.describe.after', {
				minutes: Math.round(trigger.delay_seconds / 60),
				name: parent?.name ?? '?'
			});
		}
		const seconds = trigger.every_seconds;
		const chosen: Unit =
			seconds % 86_400 === 0 ? 'days' : seconds % 3600 === 0 ? 'hours' : 'minutes';
		if (seconds === UNIT_SECONDS[chosen]) return t(`schedules.describe.everyOne.${chosen}`);
		return t('schedules.describe.every', {
			count: seconds / UNIT_SECONDS[chosen],
			unit: t(`schedules.units.${chosen}`)
		});
	}

	function describeAction(action: Action) {
		if (action.type === 'command')
			return t('schedules.describe.command', { command: action.command });
		if (action.type === 'wipe')
			return t('schedules.describe.wipe', { kind: t(`settings.wipe.${action.kind}`) });
		return t(`schedules.actions.${action.type}`);
	}

	const warningLabel = (seconds: number) =>
		seconds === 0
			? t('server.updates.warnings.none')
			: t(`server.updates.warnings.m${seconds / 60}` as MessageKey);
</script>

<svelte:head><title>{t('schedules.title')} · {server.name} · Crustation</title></svelte:head>

<div class="page space-y-4 py-4">
	<div class="flex justify-end">
		<Button onclick={openNew}>
			<PlusIcon />
			{t('schedules.new')}
		</Button>
	</div>

	{#if loadFailed}
		<Alert.Root variant="destructive">
			<Alert.Description>{t('schedules.failed')}</Alert.Description>
		</Alert.Root>
	{:else if !schedules}
		<Skeleton class="h-40 rounded-lg" />
	{:else if schedules.length === 0}
		<p class="rounded-lg border border-dashed py-12 text-center text-sm text-muted-foreground">
			{t('schedules.empty')}
		</p>
	{:else}
		<div class="overflow-x-auto rounded-lg border bg-card">
			<Table.Root>
				<Table.Header>
					<Table.Row class="bg-muted/50 hover:bg-muted/50">
						<Table.Head>{t('schedules.columns.name')}</Table.Head>
						<Table.Head>{t('schedules.columns.when')}</Table.Head>
						<Table.Head>{t('schedules.columns.what')}</Table.Head>
						<Table.Head>{t('schedules.columns.next')}</Table.Head>
						<Table.Head>{t('schedules.columns.last')}</Table.Head>
						<Table.Head class="w-16"></Table.Head>
						<Table.Head class="w-10"></Table.Head>
					</Table.Row>
				</Table.Header>
				<Table.Body>
					{#each schedules as schedule (schedule.id)}
						<Table.Row>
							<Table.Cell class="font-medium">{schedule.name}</Table.Cell>
							<Table.Cell class="font-mono text-xs">{describeTrigger(schedule.trigger)}</Table.Cell>
							<Table.Cell class="text-sm">{describeAction(schedule.action)}</Table.Cell>
							<Table.Cell class="text-xs whitespace-nowrap text-muted-foreground">
								{#if schedule.next_run_at}
									{dateTime(schedule.next_run_at)}
								{:else if schedule.enabled && schedule.trigger.type === 'after'}
									{t('schedules.waiting')}
								{:else}
									—
								{/if}
							</Table.Cell>
							<Table.Cell class="max-w-64 text-xs text-muted-foreground">
								{#if schedule.last_run_at}
									<span class="whitespace-nowrap">{dateTime(schedule.last_run_at)}</span>
									{#if schedule.last_result}
										<span
											class={[
												'block truncate',
												schedule.last_result.startsWith('Failed') && 'text-destructive'
											]}
											title={schedule.last_result}
										>
											{schedule.last_result}
										</span>
									{/if}
								{:else}
									{t('schedules.never')}
								{/if}
							</Table.Cell>
							<Table.Cell>
								<Switch
									checked={schedule.enabled}
									disabled={busy}
									aria-label={schedule.name}
									onCheckedChange={(on) =>
										act(
											() => updateSchedule(server.id, schedule.id, { enabled: on }),
											t('schedules.saved', { name: schedule.name })
										)}
								/>
							</Table.Cell>
							<Table.Cell>
								<DropdownMenu.Root>
									<DropdownMenu.Trigger>
										{#snippet child({ props })}
											<Button {...props} variant="ghost" size="icon-sm" aria-label={schedule.name}>
												<MoreIcon />
											</Button>
										{/snippet}
									</DropdownMenu.Trigger>
									<DropdownMenu.Content align="end">
										<DropdownMenu.Item
											onSelect={() =>
												act(
													() => runSchedule(server.id, schedule.id),
													t('schedules.ran', { name: schedule.name })
												)}
										>
											<PlayIcon />
											{t('schedules.runNow')}
										</DropdownMenu.Item>
										<DropdownMenu.Item onSelect={() => openEdit(schedule)}>
											<PencilIcon />
											{t('common.actions.edit')}
										</DropdownMenu.Item>
										<DropdownMenu.Separator />
										<DropdownMenu.Item variant="destructive" onSelect={() => remove(schedule)}>
											<Trash2Icon />
											{t('common.actions.delete')}
										</DropdownMenu.Item>
									</DropdownMenu.Content>
								</DropdownMenu.Root>
							</Table.Cell>
						</Table.Row>
					{/each}
				</Table.Body>
			</Table.Root>
		</div>
	{/if}
</div>

<Dialog.Root bind:open>
	<Dialog.Content class="sm:max-w-lg">
		<Dialog.Header>
			<Dialog.Title>
				{editing ? t('schedules.edit', { name: editing.name }) : t('schedules.new')}
			</Dialog.Title>
		</Dialog.Header>

		<form
			class="grid gap-4"
			onsubmit={(event) => {
				event.preventDefault();
				save();
			}}
		>
			{#if !editing}
				<div class="flex flex-wrap gap-2">
					<Button type="button" variant="outline" size="sm" onclick={() => preset('nightly')}>
						{t('schedules.presets.nightly')}
					</Button>
					{#if rust}
						<Button type="button" variant="outline" size="sm" onclick={() => preset('weekly')}>
							{t('schedules.presets.weekly')}
						</Button>
						<Button type="button" variant="outline" size="sm" onclick={() => preset('monthly')}>
							{t('schedules.presets.monthly')}
						</Button>
					{/if}
					{#if steam}
						<Button type="button" variant="outline" size="sm" onclick={() => preset('updates')}>
							{t('schedules.presets.updates')}
						</Button>
					{/if}
				</div>
			{/if}

			<div class="grid gap-2">
				<Label for="schedule-name">{t('schedules.field.name')}</Label>
				<Input id="schedule-name" bind:value={name} />
			</div>

			<div class="grid gap-2">
				<Label>{t('schedules.field.when')}</Label>
				<Select.Root type="single" bind:value={triggerType}>
					<Select.Trigger class="w-full">{t(`schedules.triggers.${triggerType}`)}</Select.Trigger>
					<Select.Content>
						{#each ['interval', 'cron', 'after'] as const as type (type)}
							<Select.Item
								value={type}
								label={t(`schedules.triggers.${type}`)}
								disabled={type === 'after' && !schedules?.some((one) => one.id !== editing?.id)}
							/>
						{/each}
					</Select.Content>
				</Select.Root>
				{#if triggerType === 'interval'}
					<div class="flex gap-2">
						<Input
							type="number"
							min="1"
							bind:value={every}
							class="w-24"
							aria-label={t('schedules.field.every')}
						/>
						<Select.Root type="single" bind:value={unit}>
							<Select.Trigger class="w-32">{t(`schedules.units.${unit}`)}</Select.Trigger>
							<Select.Content>
								{#each ['minutes', 'hours', 'days'] as const as each (each)}
									<Select.Item value={each} label={t(`schedules.units.${each}`)} />
								{/each}
							</Select.Content>
						</Select.Root>
					</div>
				{:else if triggerType === 'cron'}
					<Input bind:value={expression} class="font-mono" aria-label={t('schedules.field.cron')} />
					<p class="text-xs text-muted-foreground">{t('schedules.field.cronHint')}</p>
				{:else}
					<div class="flex gap-2">
						<Select.Root type="single" bind:value={after}>
							<Select.Trigger class="flex-1" aria-label={t('schedules.field.after')}>
								{schedules?.find((one) => one.id === after)?.name ?? t('schedules.field.after')}
							</Select.Trigger>
							<Select.Content>
								{#each schedules?.filter((one) => one.id !== editing?.id) ?? [] as other (other.id)}
									<Select.Item value={other.id} label={other.name} />
								{/each}
							</Select.Content>
						</Select.Root>
						<Input
							type="number"
							min="0"
							bind:value={delayMinutes}
							class="w-24"
							aria-label={t('schedules.field.delay')}
						/>
					</div>
				{/if}
			</div>

			<div class="grid gap-2">
				<Label>{t('schedules.field.what')}</Label>
				<Select.Root type="single" bind:value={actionType}>
					<Select.Trigger class="w-full">{t(`schedules.actions.${actionType}`)}</Select.Trigger>
					<Select.Content>
						{#each actionTypes as type (type)}
							<Select.Item value={type} label={t(`schedules.actions.${type}`)} />
						{/each}
					</Select.Content>
				</Select.Root>
				{#if actionType === 'command'}
					<Input
						bind:value={command}
						class="font-mono"
						placeholder="say Hello"
						aria-label={t('schedules.field.command')}
					/>
				{/if}
				{#if actionType === 'wipe'}
					<Select.Root type="single" bind:value={wipeKind}>
						<Select.Trigger class="w-full" aria-label={t('schedules.field.kind')}>
							{t(`settings.wipe.${wipeKind}`)}
						</Select.Trigger>
						<Select.Content>
							{#each ['map', 'blueprints', 'full'] as const as kind (kind)}
								<Select.Item value={kind} label={t(`settings.wipe.${kind}`)} />
							{/each}
						</Select.Content>
					</Select.Root>
					{#if wipeKind !== 'blueprints'}
						<Label class="font-normal">
							<Checkbox bind:checked={newSeed} />
							{t('schedules.field.newSeed')}
						</Label>
					{/if}
				{/if}
				{#if actionType === 'update'}
					<Label class="font-normal">
						<Checkbox bind:checked={onlyIfNewer} />
						{t('schedules.field.onlyIfNewer')}
					</Label>
				{/if}
				{#if warns}
					<div class="flex items-center gap-2">
						<span class="text-sm text-muted-foreground">{t('schedules.field.warn')}</span>
						<Select.Root type="single" bind:value={warn}>
							<Select.Trigger class="w-36">{warningLabel(Number(warn))}</Select.Trigger>
							<Select.Content>
								{#each WARNINGS as seconds (seconds)}
									<Select.Item value={String(seconds)} label={warningLabel(seconds)} />
								{/each}
							</Select.Content>
						</Select.Root>
					</div>
				{/if}
			</div>

			<Label class="font-normal">
				<Checkbox bind:checked={oneTime} />
				{t('schedules.field.oneTime')}
			</Label>

			<Dialog.Footer>
				<Button type="button" variant="outline" onclick={() => (open = false)}>
					{t('common.actions.cancel')}
				</Button>
				<Button type="submit" disabled={busy || !name.trim()}>{t('common.actions.save')}</Button>
			</Dialog.Footer>
		</form>
	</Dialog.Content>
</Dialog.Root>
