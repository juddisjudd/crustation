import { api } from './client';

export type PackSort = 'behaviour' | 'resource' | 'world_template' | 'skin' | 'datapack' | 'world';

export interface Pack {
	name: string;
	sort: PackSort;
	/** Where it sits, relative to the server folder. */
	path: string;
	uuid: string | null;
	version: number[];
	/** Whether the world is told to load it. */
	activated: boolean;
}

export interface World {
	/** What the world calls itself, which is what the player looks for. */
	name: string;
	/** The folder, which is what `level-name` and the pack lists key on. */
	folder: string;
	path: string;
}

export interface Installed {
	installed: Pack[];
	/** The world the packs were switched on for. */
	world: string;
	/** Set when an imported world became the one the server plays. */
	level_name: string | null;
	restart_required: boolean;
}

/** What each button offers in its file dialog. */
export const ADDON_TYPES = '.mcaddon,.mcpack,.zip';
export const WORLD_TYPES = '.mcworld,.zip';

/** The file types the panel knows how to place. */
const TAKEN = ['.mcaddon', '.mcpack', '.mcworld', '.zip'];

export const looksInstallable = (name: string) => {
	const lower = name.toLowerCase();
	return TAKEN.some((one) => lower.endsWith(one));
};

export const listPacks = (serverId: string) =>
	api.get<{ packs: Pack[] }>(`/servers/${serverId}/packs`);

export const listWorlds = (serverId: string) =>
	api.get<{ worlds: World[]; level_name: string }>(`/servers/${serverId}/worlds`);

export interface Choices {
	/** Switch the packs on for a world. Bedrock ignores one that is only there. */
	activate?: boolean;
	/** Which world to switch them on for. `level-name` when nothing is said. */
	world?: string;
	/** Whether an imported world becomes the one the server plays. */
	use_world?: boolean;
}

/** Installs a file already sitting in the server folder. */
export const installPack = (serverId: string, path: string, choices: Choices = {}) =>
	api.post<Installed>(`/servers/${serverId}/packs`, { path, ...choices });

/** Sends a file picked in a dialog straight to the panel, which unpacks it. */
export const uploadPack = (serverId: string, file: File, choices: Choices = {}) =>
	api.post<Installed>(`/servers/${serverId}/packs/upload`, file, {
		query: { name: file.name, ...choices }
	});

/** Points the server at a world it already keeps. Takes on the next start. */
export const playWorld = (serverId: string, folder: string) =>
	api.put<{ level_name: string; restart_required: boolean }>(`/servers/${serverId}/worlds`, {
		folder
	});
