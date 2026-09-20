export type GlobalPermission = 'CREATE_SERVER' | 'MANAGE_USERS' | 'MANAGE_ROLES' | 'ADMIN';

export type ServerPermission =
	'COMMANDS' | 'CONSOLE' | 'LOGS' | 'SCHEDULES' | 'BACKUPS' | 'FILES' | 'CONFIG' | 'PLAYERS';

export type ServerState =
	'stopped' | 'starting' | 'running' | 'stopping' | 'crashed' | 'installing';

export type ServerKind = 'minecraft_java' | 'minecraft_bedrock' | string;

export interface User {
	id: string;
	username: string;
	email: string | null;
	language: string;
	is_admin: boolean;
	created_at: string;
	last_login_at: string | null;
}

export interface SessionInfo {
	user: User;
	permissions: { global: GlobalPermission[] };
	panel: {
		version: string;
		docker: boolean;
		timezone: string;
		starting: boolean;
		started_at: string;
	};
}

export interface ServerStats {
	at?: string;
	cpu_percent: number;
	memory_bytes: number;
	memory_percent: number;
	players_online: number | null;
	players_max: number | null;
	version?: string | null;
	motd?: string | null;
	latency_ms?: number | null;
	started_at: string | null;
	world_size_bytes?: number | null;
}

export interface ServerFlags {
	installing: boolean;
	updating: boolean;
	backing_up: boolean;
	crashed: boolean;
	last_backup_failed: boolean;
	update_available: boolean;
}

export interface ServerSettings {
	autostart: boolean;
	autostart_delay: number;
	crash_detection: boolean;
	stop_command: string;
	shutdown_timeout: number;
	ignored_exits: string;
	count_players: boolean;
	public_status: boolean;
	log_path: string;
	directory: string;
	executable: string;
	command: string;
	java: {
		binary: string | null;
		min_memory_mb: number;
		max_memory_mb: number;
		flags: string;
	};
	provider: string | null;
	provider_version: string | null;
}

export interface Server {
	id: string;
	name: string;
	kind: ServerKind;
	created_at: string;
	address: { host: string; port: number };
	settings: ServerSettings;
	state: ServerState;
	stats: ServerStats;
	flags: ServerFlags;
	permissions: ServerPermission[];
}

export interface HostStats {
	cpu: { percent: number; cores: number };
	memory: { total_bytes: number; used_bytes: number; used_percent: number };
	disks: {
		mount: string;
		/** Which of the panel's folders live on this filesystem. */
		keeps: ('servers' | 'backups' | 'config')[];
		filesystem: string;
		total_bytes: number;
		used_bytes: number;
		free_bytes: number;
		used_percent: number;
	}[];
	servers: { total: number; running: number; stopped: number };
	uptime_seconds: number;
}

export interface ConsoleLine {
	seq: number;
	at: string;
	/** `command` and `rcon` are the panel's own echo of an RCON exchange. */
	stream: 'stdout' | 'stderr' | 'command' | 'rcon';
	text: string;
}

export interface CommandResult {
	via: 'rcon' | 'stdin';
	output: string | null;
}

export interface RconStatus {
	/** Java speaks RCON; Bedrock does not. */
	supported: boolean;
	enabled: boolean;
	port: number | null;
	/** Proven by connecting, so only ever true while the server is up. */
	reachable: boolean;
}
