import { toast } from 'svelte-sonner';
import { api, ApiError } from './client';
import type { CommandResult, RconStatus } from './types';

export type PowerAction = 'start' | 'stop' | 'restart' | 'kill';

const pending: Record<PowerAction, string> = {
	start: 'Starting',
	stop: 'Stopping',
	restart: 'Restarting',
	kill: 'Killing'
};

export function errorMessage(err: unknown, fallback = 'Something went wrong') {
	if (err instanceof ApiError) return err.message || err.code;
	return err instanceof Error ? err.message : fallback;
}

export async function powerAction(serverId: string, action: PowerAction, serverName = 'server') {
	try {
		await api.post(`/servers/${serverId}/action`, { action });
		toast(`${pending[action]} ${serverName}…`);
		return true;
	} catch (err) {
		toast.error(`Could not ${action} ${serverName}`, { description: errorMessage(err) });
		return false;
	}
}

export function sendCommand(serverId: string, command: string) {
	return api.post<CommandResult>(`/servers/${serverId}/command`, { command });
}

export function clearConsole(serverId: string) {
	return api.delete<void>(`/servers/${serverId}/console`);
}

export function rconStatus(serverId: string) {
	return api.get<RconStatus>(`/servers/${serverId}/rcon`);
}

export function enableRcon(serverId: string) {
	return api.post<{ port: number; restart_required: boolean }>(`/servers/${serverId}/rcon`, {});
}

export interface RecentPoint {
	cpu: number;
	memory_percent: number;
}

/** Recent history for every server the caller can see, keyed by server id. */
export const recentStats = (minutes = 30, points = 32) =>
	api.get<{ since: string; minutes: number; series: Record<string, (RecentPoint | null)[]> }>(
		'/servers/stats',
		{ query: { minutes, points } }
	);

/** Removes a server. It has to be stopped first, and the files are optional. */
export const deleteServer = (serverId: string, deleteFiles: boolean) =>
	api.delete(`/servers/${serverId}`, undefined, { query: { delete_files: deleteFiles } });
