import type { Snippet } from 'svelte';

export interface ConfirmOptions {
	title: string;
	description?: string | Snippet;
	confirmLabel?: string;
	cancelLabel?: string;
	destructive?: boolean;
	/** Extra checkbox shown above the buttons; its value is returned as `checked`. */
	checkbox?: string;
}

interface Pending extends ConfirmOptions {
	resolve: (result: { confirmed: boolean; checked: boolean }) => void;
}

class ConfirmState {
	current = $state<Pending | null>(null);
	open = $state(false);
	checked = $state(false);

	ask(options: ConfirmOptions) {
		this.current?.resolve({ confirmed: false, checked: false });
		return new Promise<{ confirmed: boolean; checked: boolean }>((resolve) => {
			this.checked = false;
			this.current = { ...options, resolve };
			this.open = true;
		});
	}

	settle(confirmed: boolean) {
		this.current?.resolve({ confirmed, checked: confirmed && this.checked });
		this.current = null;
		this.open = false;
	}
}

export const confirmState = new ConfirmState();

export async function confirm(options: ConfirmOptions) {
	return (await confirmState.ask(options)).confirmed;
}

/** Same as confirm(), but also returns the state of the optional checkbox. */
export function confirmWith(options: ConfirmOptions) {
	return confirmState.ask(options);
}
