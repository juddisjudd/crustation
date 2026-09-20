<script lang="ts">
	import * as AlertDialog from '$lib/components/ui/alert-dialog/index.js';
	import { Checkbox } from '$lib/components/ui/checkbox/index.js';
	import { Label } from '$lib/components/ui/label/index.js';
	import { confirmState } from './confirm.svelte';
	import { t } from '$lib/i18n/index.svelte';

	const current = $derived(confirmState.current);
</script>

<AlertDialog.Root
	bind:open={() => confirmState.open, (open) => !open && confirmState.settle(false)}
>
	<AlertDialog.Content>
		{#if current}
			<AlertDialog.Header>
				<AlertDialog.Title>{current.title}</AlertDialog.Title>
				{#if current.description}
					<AlertDialog.Description>
						{#if typeof current.description === 'string'}
							{current.description}
						{:else}
							{@render current.description()}
						{/if}
					</AlertDialog.Description>
				{/if}
			</AlertDialog.Header>
			{#if current.checkbox}
				<div class="flex items-center gap-2">
					<Checkbox id="confirm-checkbox" bind:checked={confirmState.checked} />
					<Label for="confirm-checkbox" class="font-normal">{current.checkbox}</Label>
				</div>
			{/if}
			<AlertDialog.Footer>
				<AlertDialog.Cancel onclick={() => confirmState.settle(false)}>
					{current.cancelLabel ?? t('common.actions.cancel')}
				</AlertDialog.Cancel>
				<AlertDialog.Action
					variant={current.destructive ? 'destructive' : 'default'}
					onclick={() => confirmState.settle(true)}
				>
					{current.confirmLabel ?? t('common.actions.confirm')}
				</AlertDialog.Action>
			</AlertDialog.Footer>
		{/if}
	</AlertDialog.Content>
</AlertDialog.Root>
