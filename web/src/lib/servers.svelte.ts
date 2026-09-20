import { SvelteMap } from 'svelte/reactivity';
import { api } from '$lib/api/client';
import type { Server, ServerState } from '$lib/api/types';
import { socket } from '$lib/realtime/socket.svelte';
import { t, type MessageKey } from '$lib/i18n/index.svelte';

export type ServerStatus = ServerState | 'unknown';

export function statusLabel(status: ServerStatus) {
	return t(`common.server.status.${status}` as MessageKey);
}

interface LiveStats {
	server_id: string;
	cpu_percent?: number;
	memory_bytes?: number;
	memory_percent?: number;
	players_online?: number | null;
	players_max?: number | null;
}

class Servers {
	list = $state.raw<Server[]>([]);
	loaded = $state(false);
	/** Latest realtime numbers, which win over the snapshot from the list. */
	live = new SvelteMap<string, LiveStats>();
	states = new SvelteMap<string, ServerState>();

	#refreshing: Promise<void> | null = null;

	refresh() {
		this.#refreshing ??= api
			.get<Server[]>('/servers')
			.then((list) => {
				this.list = list;
				this.loaded = true;
				for (const server of list) this.states.set(server.id, server.state);
			})
			.finally(() => {
				this.#refreshing = null;
			});
		return this.#refreshing;
	}

	/** Keeps the list in step with the panel while the app is open. */
	listen() {
		const stops = [
			socket.on<{ server_id: string; state: ServerState }>('servers', 'state', (event) => {
				if (event?.server_id) this.states.set(event.server_id, event.state);
			}),
			socket.on<LiveStats>('servers', 'stats', (stats) => {
				if (stats?.server_id) {
					this.live.set(stats.server_id, { ...this.live.get(stats.server_id), ...stats });
				}
			}),
			socket.on('servers', 'created', () => this.refresh()),
			socket.on('servers', 'deleted', () => this.refresh())
		];
		return () => stops.forEach((stop) => stop());
	}

	get(id: string) {
		return this.list.find((server) => server.id === id);
	}

	statusOf(id: string): ServerStatus {
		return this.states.get(id) ?? this.get(id)?.state ?? 'unknown';
	}

	isRunning(id: string) {
		return this.statusOf(id) === 'running';
	}

	metrics(id: string) {
		const server = this.get(id);
		const live = this.live.get(id);
		const stats = server?.stats;
		const running = this.isRunning(id);
		return {
			running,
			cpu: running ? (live?.cpu_percent ?? stats?.cpu_percent ?? 0) : 0,
			memoryBytes: running ? (live?.memory_bytes ?? stats?.memory_bytes ?? 0) : 0,
			memPercent: running ? (live?.memory_percent ?? stats?.memory_percent ?? 0) : 0,
			online: running ? (live?.players_online ?? stats?.players_online ?? 0) : 0,
			max: live?.players_max ?? stats?.players_max ?? 0,
			version: stats?.version ?? '',
			motd: stats?.motd ?? '',
			started: running ? (stats?.started_at ?? null) : null,
			worldSizeBytes: stats?.world_size_bytes ?? 0,
			port: server?.address.port
		};
	}

	get totals() {
		let running = 0;
		let players = 0;
		let maxPlayers = 0;
		for (const server of this.list) {
			if (this.isRunning(server.id)) running++;
			if (!server.settings.count_players) continue;
			const metrics = this.metrics(server.id);
			players += metrics.online ?? 0;
			maxPlayers += metrics.max ?? 0;
		}
		return {
			total: this.list.length,
			running,
			stopped: this.list.length - running,
			players,
			maxPlayers
		};
	}
}

export const servers = new Servers();
