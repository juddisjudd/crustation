import { api } from './client';

export type WebMap =
	| { found: false }
	| {
			found: true;
			id: 'squaremap' | 'pl3xmap' | 'bluemap' | 'dynmap';
			name: string;
			port: number;
			enabled: boolean;
			/** Something is listening on that port right now. */
			answering: boolean;
			config: string;
	  };

export interface Spot {
	name: string;
	x: number;
	y: number;
	z: number;
	dimension: string;
}

export interface Positions {
	supported: boolean;
	reason?: string;
	players: Spot[];
}

export const webMap = (serverId: string) => api.get<WebMap>(`/servers/${serverId}/map`);

export const positions = (serverId: string) => api.get<Positions>(`/servers/${serverId}/positions`);
