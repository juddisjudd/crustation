import { error } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import { session } from '$lib/session.svelte';

export async function load({ parent }) {
	// The layout load is what fetches the session, so wait for it before asking.
	await parent();
	if (!session.can('MANAGE_USERS')) error(403, 'You cannot manage users.');
	return {
		crumbs: [
			{ labelKey: 'nav.overview' as const, href: resolve('/') },
			{ labelKey: 'nav.users' as const }
		]
	};
}
