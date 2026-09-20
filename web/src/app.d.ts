// See https://svelte.dev/docs/kit/types#app.d.ts
// for information about these interfaces
import type { MessageKey } from '$lib/i18n/index.svelte';

declare global {
	namespace App {
		interface PageData {
			/** Give `labelKey` for translated crumbs; `label` is for names such as a server's. */
			crumbs?: { label?: string; labelKey?: MessageKey; href?: string }[];
		}
	}
}

export {};
