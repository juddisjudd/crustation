import { api } from './client';

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
}

export interface PlayerList {
	list: string;
	file: string;
	/** The field that identifies a row: name, xuid or ip. */
	key: string;
	entries: Record<string, unknown>[];
}

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

/** A head render, from the community avatar service Minecraft panels use. */
export const headUrl = (player: { uuid?: string | null; name: string }) =>
	`https://mc-heads.net/avatar/${encodeURIComponent(player.uuid || player.name)}/32`;
