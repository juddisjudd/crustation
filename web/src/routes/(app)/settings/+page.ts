import { error } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import { session } from '$lib/session.svelte';

export async function load({ parent }) {
	// The layout load is what fetches the session, so wait for it before asking.
	await parent();
	if (!session.superuser) error(403, 'Only an administrator can change panel settings.');
	return {
		crumbs: [
			{ labelKey: 'nav.overview' as const, href: resolve('/') },
			{ labelKey: 'nav.settings' as const }
		]
	};
}
