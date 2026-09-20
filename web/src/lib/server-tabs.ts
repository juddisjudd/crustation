import type { Server, ServerPermission } from '$lib/api/types';
import type { MessageKey } from '$lib/i18n/index.svelte';

export interface ServerTab {
	slug: string;
	label: MessageKey;
	permission?: ServerPermission;
}

export const serverTabs: ServerTab[] = [
	{ slug: '', label: 'nav.tabs.overview' },
	{ slug: 'console', label: 'nav.tabs.console', permission: 'CONSOLE' },
	{ slug: 'files', label: 'nav.tabs.files', permission: 'FILES' },
	{ slug: 'players', label: 'nav.tabs.players', permission: 'PLAYERS' },
	{ slug: 'settings', label: 'nav.tabs.settings', permission: 'CONFIG' }
];

export function visibleTabs(server: Server, isAdmin: boolean) {
	return serverTabs.filter(
		(tab) => !tab.permission || isAdmin || server.permissions.includes(tab.permission)
	);
}
