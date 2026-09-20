import { toast } from 'svelte-sonner';
import { api, ApiError } from './client';

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
	return api.post(`/servers/${serverId}/command`, { command });
}
