import { api, API_ROOT } from './client';

export interface RustOnline {
	steam_id: string;
	name: string;
	ping: number;
	address: string;
	connected_seconds: number;
	health: number;
}

export interface RustKnown {
	steam_id: string | null;
	name: string;
	first_seen: string;
	last_seen: string;
	online: boolean;
}

export type RustRole = 'owner' | 'moderator';

export interface RustStaff {
	steam_id: string;
	name: string;
	role: RustRole;
}

export interface RustBan {
	steam_id: string;
	name: string;
	reason: string;
	/** Unix seconds; null for a ban that never ends. */
	expires: number | null;
}

export interface RustPlayers {
	running: boolean;
	online: RustOnline[];
	known: RustKnown[];
	staff: RustStaff[];
	bans: RustBan[];
}

export type RustPlayerAction =
	| { action: 'kick'; steam_id: string; reason?: string }
	| { action: 'ban'; steam_id: string; name?: string; reason?: string; duration?: string }
	| { action: 'unban'; steam_id: string }
	| { action: 'staff'; steam_id: string; name?: string; role: RustRole | null }
	| { action: 'mute'; steam_id: string; duration?: string }
	| { action: 'unmute'; steam_id: string }
	| { action: 'say'; message: string }
	| { action: 'teleport'; steam_id: string; to: string }
	| { action: 'give'; steam_id: string; item: string; amount?: number }
	| { action: 'give-all'; item: string; amount?: number }
	| { action: 'heal' | 'revive' | 'kill' | 'clear-inventory'; steam_id: string }
	| { action: 'heal-all' };

export interface RustActed {
	via: 'rcon' | 'file';
	ran: string[];
	output?: string;
}

export interface RustChatLine {
	channel: number;
	message: string;
	user_id: string;
	username: string;
	time: number;
}

export const rustPlayers = (serverId: string) =>
	api.get<RustPlayers>(`/servers/${serverId}/rust/players`);

export const actOnRustPlayer = (serverId: string, action: RustPlayerAction) =>
	api.post<RustActed>(`/servers/${serverId}/rust/player-actions`, action);

export const rustChat = (serverId: string) =>
	api.get<{ running: boolean; lines: RustChatLine[] }>(`/servers/${serverId}/rust/chat`);

export type RustWipe = 'map' | 'blueprints' | 'full';

export const wipeRust = (serverId: string, kind: RustWipe, newSeed: boolean) =>
	api.post<{ deleted: string[]; seed: number | null }>(`/servers/${serverId}/rust/wipe`, {
		kind,
		new_seed: newSeed
	});

export interface RustBuild {
	branch: string;
	installed: number | null;
	latest: number | null;
	update_available: boolean;
}

export const rustBuild = (serverId: string) =>
	api.get<RustBuild>(`/servers/${serverId}/rust/build`);

/** `warn` is how many seconds players get, counted down in chat, before a running server stops. */
export const updateRust = (serverId: string, warn: number) =>
	api.post(`/servers/${serverId}/rust/build`, { warn });

export type RustConvarType = 'bool' | 'int' | 'float' | 'string' | 'vector';

export interface RustConvar {
	name: string;
	help: string;
	/** Rust's help for this one was written by a tool rather than by hand. */
	generated: boolean;
	type: RustConvarType;
	default: string | null;
	/** What the running server holds; null while it is down. */
	value: string | null;
	/** What the start command sets; null when it says nothing. */
	startup: string | null;
	managed: boolean;
}

export interface RustConvars {
	running: boolean;
	/** The list and values came from the running server rather than Carbon's reference. */
	live: boolean;
	can_keep: boolean;
	convars: RustConvar[];
}

export interface RustConvarSet {
	live: string | null;
	startup: string | null;
	restart_required: boolean;
}

export interface RustCommand {
	name: string;
	help: string;
	generated: boolean;
}

export const rustConvars = (serverId: string) =>
	api.get<RustConvars>(`/servers/${serverId}/rust/convars`);

/** `now` tells the running server; `startup` keeps it on the start command, or takes it off with null. */
export const setRustConvar = (
	serverId: string,
	name: string,
	body: { value: string | null; now?: boolean; startup?: boolean }
) => api.put<RustConvarSet>(`/servers/${serverId}/rust/convars/${encodeURIComponent(name)}`, body);

export const rustCommands = (serverId: string) =>
	api.get<{ running: boolean; live: boolean; commands: RustCommand[] }>(
		`/servers/${serverId}/rust/commands`
	);

export const isSteamId = (value: string) => /^7656\d{13}$/.test(value.trim());

export const steamProfileUrl = (steamId: string) =>
	`https://steamcommunity.com/profiles/${steamId}`;

export type RustFramework = 'oxide' | 'carbon';

export interface RustPlugin {
	file: string;
	name: string;
	title: string | null;
	author: string | null;
	version: string | null;
	enabled: boolean;
}

export interface RustMods {
	framework: RustFramework | null;
	/** What the framework says about itself over RCON; only while the server runs. */
	version: string | null;
	running: boolean;
	plugins: RustPlugin[];
}

export interface UmodPlugin {
	name: string;
	title: string;
	description: string;
	author: string;
	version: string;
	downloads: number;
	url: string;
}

const mods = (serverId: string) => `/servers/${serverId}/rust`;

export const rustMods = (serverId: string) => api.get<RustMods>(`${mods(serverId)}/mods`);

export const switchFramework = (serverId: string, framework: RustFramework | null) =>
	api.put(`${mods(serverId)}/mods`, { framework });

export const uploadPlugin = (serverId: string, file: File) =>
	api.post(`${mods(serverId)}/plugins?name=${encodeURIComponent(file.name)}`, file);

export const togglePlugin = (serverId: string, file: string, enabled: boolean) =>
	api.patch(`${mods(serverId)}/plugins/${encodeURIComponent(file)}`, { enabled });

export const removePlugin = (serverId: string, file: string) =>
	api.delete(`${mods(serverId)}/plugins/${encodeURIComponent(file)}`);

export const searchUmod = (serverId: string, query: string, page = 1) =>
	api.get<{ plugins: UmodPlugin[]; page: number; last_page: number; total: number }>(
		`${mods(serverId)}/plugins/search`,
		{ query: { q: query, page: String(page) } }
	);

export const installFromUmod = (serverId: string, name: string) =>
	api.post(`${mods(serverId)}/plugins/install`, { name });

export const pluginUpdates = (serverId: string) =>
	api.get<{ latest: Record<string, string> }>(`${mods(serverId)}/plugins/updates`);

export interface RustHealth {
	framerate: number;
	entities: number;
	queued: number;
	joining: number;
	/** Bytes a second, as the server counts them. */
	network_in: number;
	network_out: number;
	game_time: string;
}

export interface RustPlus {
	state: 'off' | 'failed' | 'listening' | 'unknown';
	/** What Rust said when its connectivity test failed. */
	detail: string | null;
	listening: string | null;
	connects_to: string | null;
}

export const rustHealth = (serverId: string) =>
	api.get<{ health: RustHealth | null; rust_plus: RustPlus; map_image: string | null }>(
		`/servers/${serverId}/rust/health`
	);

export interface RustReport {
	id: string;
	received_at: string;
	reporter_id: string;
	reporter_name: string | null;
	/** 0 general, 1 bug, 2 cheat, 3 abuse, 4 idea. */
	kind: number;
	subject: string;
	message: string;
	target_id: string | null;
	target_name: string | null;
	has_image: boolean;
}

export const rustReports = (serverId: string) =>
	api.get<{ enabled: boolean; reports: RustReport[] }>(`/servers/${serverId}/rust/reports`);

export const setReportInbox = (serverId: string, enabled: boolean) =>
	api.put(`/servers/${serverId}/rust/reports`, { enabled });

export const deleteReport = (serverId: string, reportId: string) =>
	api.delete(`/servers/${serverId}/rust/reports/${reportId}`);

export const reportImageUrl = (serverId: string, reportId: string) =>
	`${API_ROOT}/servers/${serverId}/rust/reports/${reportId}/image`;

export interface SharedBan {
	steam_id: string;
	name: string;
	reason: string;
	/** Unix seconds; null for a ban that never ends. */
	expires_at: number | null;
	mute: boolean;
	created_at: string;
	created_by: string | null;
}

export const sharedBans = (serverId: string) =>
	api.get<{ enabled: boolean; can_edit: boolean; bans: SharedBan[] }>(
		`/servers/${serverId}/rust/shared-bans`
	);

export const setSharedBans = (serverId: string, enabled: boolean) =>
	api.put(`/servers/${serverId}/rust/shared-bans`, { enabled });

export const addSharedBan = (ban: {
	steam_id: string;
	name?: string;
	reason?: string;
	duration?: string;
	mute: boolean;
}) => api.post('/rust-bans', ban);

export const liftSharedBan = (steamId: string) => api.delete(`/rust-bans/${steamId}`);
