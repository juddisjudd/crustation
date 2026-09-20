import { api } from './client';

export type PackSort = 'behaviour' | 'resource' | 'world_template' | 'skin' | 'datapack';

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

/** The file types the panel knows how to place. */
const TAKEN = ['.mcaddon', '.mcpack', '.mcworld', '.zip'];

export const looksInstallable = (name: string) => {
	const lower = name.toLowerCase();
	return TAKEN.some((one) => lower.endsWith(one));
};

export const listPacks = (serverId: string) =>
	api.get<{ packs: Pack[] }>(`/servers/${serverId}/packs`);

export const installPack = (serverId: string, path: string, activate = true) =>
	api.post<{ installed: Pack[]; restart_required: boolean }>(`/servers/${serverId}/packs`, {
		path,
		activate
	});
