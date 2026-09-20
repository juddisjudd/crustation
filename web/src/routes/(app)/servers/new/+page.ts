import { error } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import { ApiError } from '$lib/api/client';
import { listJavaRuntimes, listProviders } from '$lib/api/create';

export async function load() {
	try {
		const [providers, java] = await Promise.all([
			listProviders(),
			// Missing Java is worth saying on the page, not worth failing the load over.
			listJavaRuntimes().catch(() => [])
		]);
		return {
			providers,
			java,
			crumbs: [
				{ labelKey: 'nav.overview' as const, href: resolve('/') },
				{ labelKey: 'nav.newServer' as const }
			]
		};
	} catch (err) {
		if (err instanceof ApiError && err.status === 403) {
			error(403, 'You cannot create servers.');
		}
		throw err;
	}
}
