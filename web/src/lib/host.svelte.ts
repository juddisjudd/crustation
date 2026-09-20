import { api } from '$lib/api/client';
import type { HostStats } from '$lib/api/types';
import { socket } from '$lib/realtime/socket.svelte';

class Host {
	stats = $state.raw<HostStats | null>(null);

	async load() {
		this.stats = await api.get<HostStats>('/panel/stats');
	}

	/** The panel pushes cheap updates between full reads. */
	listen() {
		return socket.on<{
			cpu_percent: number;
			memory_total_bytes: number;
			memory_used_bytes: number;
			memory_used_percent: number;
		}>('panel', 'host_stats', (event) => {
			if (!this.stats || !event) return;
			this.stats = {
				...this.stats,
				cpu: { ...this.stats.cpu, percent: event.cpu_percent },
				memory: {
					total_bytes: event.memory_total_bytes,
					used_bytes: event.memory_used_bytes,
					used_percent: event.memory_used_percent
				}
			};
		});
	}
}

export const host = new Host();
