import { redirect } from '@sveltejs/kit';
import { base } from '$app/paths';
import { ApiError } from '$lib/api/client';
import { session } from '$lib/session.svelte';

export async function load({ url }) {
	try {
		return { session: session.info ?? (await session.load()) };
	} catch (err) {
		if (err instanceof ApiError && err.status === 401) {
			redirect(307, `${base}/login?next=${encodeURIComponent(url.pathname + url.search)}`);
		}
		throw err;
	}
}
