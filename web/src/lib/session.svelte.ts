import { base } from '$app/paths';
import { api } from '$lib/api/client';
import type { GlobalPermission, SessionInfo } from '$lib/api/types';
import { socket } from '$lib/realtime/socket.svelte';

class Session {
	info = $state.raw<SessionInfo | null>(null);

	get user() {
		return this.info?.user ?? null;
	}

	get superuser() {
		return this.info?.user.is_admin ?? false;
	}

	async load() {
		this.info = await api.get<SessionInfo>('/auth/session');
		return this.info;
	}

	can(permission: GlobalPermission) {
		return this.superuser || !!this.info?.permissions.global.includes(permission);
	}

	async logout() {
		socket.disconnect();
		try {
			await api.post('/auth/logout', undefined, { allowAnonymous: true });
		} catch {
			// signing out locally is what matters
		}
		this.info = null;
		location.replace(`${base}/login`);
	}
}

export const session = new Session();
