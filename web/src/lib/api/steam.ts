import { api } from './client';

export interface SteamBuild {
	branch: string;
	installed: number | null;
	latest: number | null;
	update_available: boolean;
}

export const steamBuild = (serverId: string) =>
	api.get<SteamBuild>(`/servers/${serverId}/steam/build`);

/** `warn` is how many seconds players get, counted down in chat, before a running server stops. */
export const updateFromSteam = (serverId: string, warn: number) =>
	api.post(`/servers/${serverId}/steam/build`, { warn });

export interface RosterPlayer {
	name: string;
	/** Dragonwilds' Epic account id; Enshrouded names nobody by id. */
	id: string | null;
	first_seen: string;
	last_seen: string;
	online: boolean;
}

export interface Roster {
	running: boolean;
	count: number;
	max: number | null;
	known: RosterPlayer[];
}

export const readRoster = (serverId: string) => api.get<Roster>(`/servers/${serverId}/roster`);

export interface EnshroudedRole {
	name: string;
	password: string;
	canKickBan: boolean;
	canAccessInventories: boolean;
	canEditWorld: boolean;
	canEditBase: boolean;
	canExtendBase: boolean;
	reservedSlots: number;
	[other: string]: unknown;
}

export const ROLE_PERMISSIONS = [
	'canKickBan',
	'canAccessInventories',
	'canEditWorld',
	'canEditBase',
	'canExtendBase'
] as const;

export const readRoles = (serverId: string) =>
	api.get<{ running: boolean; slots: number; roles: EnshroudedRole[] }>(
		`/servers/${serverId}/enshrouded/roles`
	);

export const saveRoles = (serverId: string, roles: EnshroudedRole[]) =>
	api.put<{ roles: EnshroudedRole[] }>(`/servers/${serverId}/enshrouded/roles`, { roles });

export interface EnshroudedBan {
	index: number;
	name: string | null;
	character: string | null;
	/** Unix seconds. */
	banned_at: number | null;
}

export const readBans = (serverId: string) =>
	api.get<{ running: boolean; bans: EnshroudedBan[] }>(`/servers/${serverId}/enshrouded/bans`);

export const liftBan = (serverId: string, index: number) =>
	api.delete(`/servers/${serverId}/enshrouded/bans/${index}`);
