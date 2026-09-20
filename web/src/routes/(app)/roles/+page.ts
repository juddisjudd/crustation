import { error } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import { session } from '$lib/session.svelte';

export async function load({ parent }) {
	// The layout load is what fetches the session, so wait for it before asking.
	await parent();
	if (!session.can('MANAGE_ROLES')) error(403, 'You cannot manage roles.');
	return {
		crumbs: [
			{ labelKey: 'nav.overview' as const, href: resolve('/') },
			{ labelKey: 'nav.roles' as const }
		]
	};
}
