import { api } from './client';

export interface BridgeStatus {
	supported: boolean;
	installed: boolean;
	/** Heard from within the last few seconds. */
	connected: boolean;
	last_seen: string | null;
	/** Null when there is no world yet, which is not the same as off. */
	beta_apis: boolean | null;
	world: string;
	/** Where the panel thinks the game server can reach it. */
	suggested_url: string;
}

export interface Installed {
	panel_url: string;
	world: string;
	beta_apis_turned_on: boolean;
	world_missing: boolean;
	restart_required: boolean;
}

const at = (serverId: string) => `/servers/${serverId}/bridge`;

export const bridgeStatus = (serverId: string) => api.get<BridgeStatus>(at(serverId));

export const installBridge = (serverId: string, panelUrl?: string) =>
	api.post<Installed>(at(serverId), panelUrl ? { panel_url: panelUrl } : {});

export const removeBridge = (serverId: string) => api.delete(at(serverId));
