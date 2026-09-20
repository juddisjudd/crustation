import { error } from '@sveltejs/kit';
import { resolve } from '$app/paths';
import { api, ApiError } from '$lib/api/client';
import type { Server } from '$lib/api/types';

export async function load({ params, depends }) {
	depends('crustation:server');
	try {
		const server = await api.get<Server>(`/servers/${params.id}`);
		return {
			server,
			crumbs: [
				{ labelKey: 'nav.servers' as const, href: resolve('/') },
				{ label: server.name, href: resolve(`/servers/${server.id}`) }
			]
		};
	} catch (err) {
		if (err instanceof ApiError && err.status === 404) error(404, 'Server not found');
		throw err;
	}
}
