import type { RustBan, RustRole } from '$lib/api/rust';

/** One row on the players page: on now, seen before, or only named on a list. */
export interface Person {
	steamId: string | null;
	name: string;
	on: boolean;
	ping?: number;
	health?: number;
	address?: string;
	connectedSeconds?: number;
	first?: string;
	last?: string;
	role?: RustRole;
	banned: boolean;
	ban?: RustBan;
}

export type Verb =
	| 'kick'
	| 'ban'
	| 'unban'
	| 'mute'
	| 'unmute'
	| 'give'
	| 'teleport'
	| 'heal'
	| 'revive'
	| 'kill'
	| 'clear-inventory';
