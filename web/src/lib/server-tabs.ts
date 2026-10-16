import type { Server, ServerPermission } from '$lib/api/types';
import { countsFromConsole, isMinecraft } from '$lib/format';
import type { MessageKey } from '$lib/i18n/index.svelte';

export interface ServerTab {
	slug: string;
	label: MessageKey;
	permission?: ServerPermission;
	/** Needs something only a Minecraft server has: packs and worlds. */
	minecraftOnly?: boolean;
	/** Oxide and Carbon plugins. */
	rustOnly?: boolean;
	/** Needs chat the panel can read, which Enshrouded and Dragonwilds never print. */
	chat?: boolean;
}

export const serverTabs: ServerTab[] = [
	{ slug: '', label: 'nav.tabs.overview' },
	{ slug: 'console', label: 'nav.tabs.console', permission: 'CONSOLE' },
	{ slug: 'files', label: 'nav.tabs.files', permission: 'FILES' },
	{ slug: 'content', label: 'nav.tabs.content', permission: 'FILES', minecraftOnly: true },
	{ slug: 'plugins', label: 'nav.tabs.plugins', permission: 'FILES', rustOnly: true },
	{ slug: 'players', label: 'nav.tabs.players', permission: 'PLAYERS' },
	{ slug: 'admin', label: 'nav.tabs.admin', permission: 'COMMANDS', rustOnly: true },
	{ slug: 'chat', label: 'nav.tabs.chat', permission: 'CONSOLE', chat: true },
	{ slug: 'schedules', label: 'nav.tabs.schedules', permission: 'SCHEDULES' },
	{ slug: 'settings', label: 'nav.tabs.settings', permission: 'CONFIG' }
];

export function visibleTabs(server: Server, isAdmin: boolean) {
	return serverTabs.filter(
		(tab) =>
			(!tab.minecraftOnly || isMinecraft(server.kind)) &&
			(!tab.rustOnly || server.kind === 'rust') &&
			(!tab.chat || !countsFromConsole(server.kind)) &&
			(!tab.permission || isAdmin || server.permissions.includes(tab.permission))
	);
}
