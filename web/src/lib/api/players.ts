import { api, API_ROOT } from './client';

export interface OnlinePlayer {
	name: string;
	uuid: string | null;
}

export interface KnownPlayer {
	name: string;
	uuid: string | null;
	first_seen: string;
	last_seen: string;
	online: boolean;
}

export interface PlayerOverview {
	online: OnlinePlayer[] | null;
	count: number | null;
	max: number | null;
	/** Java reports a sample rather than the whole list, so say so. */
	sampled: boolean;
	known: KnownPlayer[];
	lists: string[];
	/** Lower-cased names, so a row can show who holds what. */
	operators: string[];
	banned: string[];
	/** Everybody named on any list, so the page can act on somebody unseen. */
	listed: string[];
	running: boolean;
	edition: 'java' | 'bedrock';
}

export interface PlayerList {
	list: string;
	file: string;
	/** The field that identifies a row: name, xuid or ip. */
	key: string;
	entries: Record<string, unknown>[];
}

export interface Acted {
	/** "rcon", "stdin", or "file" when the server was down. */
	via: string;
	ran: string;
	output?: string | null;
	restart_required: boolean;
}

export interface GiveableItem {
	/** Without a namespace for anything vanilla, as both editions read it. */
	id: string;
	name: string;
	/** A creative tab, or `other` for Bedrock-only and add-on items. */
	category: string;
	/** Set when a picture exists. The panel serves it; this is only the flag. */
	icon?: string;
}

export interface ItemCatalogue {
	/** `server` when the add-on said what it holds, `catalogue` when the panel
	 * is offering the vanilla list because nothing could be asked. */
	source: 'server' | 'catalogue';
	kind: string;
	items: GiveableItem[];
}

export type Spot = string | { x: number; y: number; z: number };

export type PlayerAction =
	| { action: 'op'; player: string }
	| { action: 'deop'; player: string }
	| { action: 'kick'; player: string; reason?: string }
	| { action: 'ban'; player: string; reason?: string }
	| { action: 'pardon'; player: string }
	| { action: 'rank'; player: string; rank: string }
	| { action: 'give'; player: string; item: string; count?: number }
	| { action: 'teleport'; player: string; to: Spot }
	| { action: 'say'; message: string }
	| { action: 'whisper'; player: string; message: string };

const base = (serverId: string) => `/servers/${serverId}/players`;

export const playerOverview = (serverId: string) => api.get<PlayerOverview>(base(serverId));

export const readList = (serverId: string, list: string) =>
	api.get<PlayerList>(`${base(serverId)}/${list}`);

export const addToList = (
	serverId: string,
	list: string,
	value: string,
	extra: { reason?: string; level?: number } = {}
) => api.post(`${base(serverId)}/${list}`, { value, ...extra });

export const removeFromList = (serverId: string, list: string, value: string) =>
	api.delete(`${base(serverId)}/${list}`, { value });

export const actOnPlayer = (serverId: string, action: PlayerAction) =>
	api.post<Acted>(`/servers/${serverId}/player-actions`, action);

export const giveableItems = (serverId: string) =>
	api.get<ItemCatalogue>(`/servers/${serverId}/items`);

/** The panel's own copy, fetched from the gallery once and then kept. */
export const itemIconUrl = (item: GiveableItem) =>
	item.icon ? `${API_ROOT}/items/${encodeURIComponent(item.id)}/icon` : null;

/** A head render, from the community avatar service Minecraft panels use. */
export const headUrl = (player: { uuid?: string | null; name: string }) =>
	`https://mc-heads.net/avatar/${encodeURIComponent(player.uuid || player.name)}/32`;
