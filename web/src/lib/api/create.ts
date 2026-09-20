import { api } from './client';
import type { ServerKind } from './types';

export interface Provider {
	id: string;
	name: string;
	kind: ServerKind;
	summary: string;
	default_port: number;
	needs_java: boolean;
}

export interface ProviderVersion {
	id: string;
	label: string;
	stable: boolean;
}

export interface JavaRuntime {
	path: string;
	version: string;
	major: number | null;
}

/** A folder inside an uploaded archive that looks like a server. */
export interface ArchiveRoot {
	path: string;
	files: string[];
}

export type CreateSource =
	| { type: 'provider'; provider: string; version: string }
	| { type: 'url'; kind: ServerKind; url: string; executable?: string }
	| {
			type: 'zip';
			kind: ServerKind;
			upload_id: string;
			internal_path?: string;
			executable?: string;
	  }
	| { type: 'folder'; kind: ServerKind; path: string; executable?: string };

export interface NewServer {
	name: string;
	host?: string;
	port?: number;
	min_memory_mb?: number;
	max_memory_mb?: number;
	java_binary?: string | null;
	java_flags?: string;
	autostart?: boolean;
	agree_to_eula: boolean;
	source: CreateSource;
}

export type InstallPhase =
	| 'resolving'
	| 'downloading'
	| 'extracting'
	| 'copying'
	| 'installing'
	| 'configuring'
	| 'done'
	| 'failed';

export interface InstallProgress {
	server_id: string;
	phase: InstallPhase;
	percent: number | null;
	message: string;
}

export const listProviders = () => api.get<Provider[]>('/providers');

export const listVersions = (provider: string) =>
	api.get<ProviderVersion[]>(`/providers/${provider}/versions`);

export const listJavaRuntimes = () =>
	api.get<{ runtimes: JavaRuntime[] }>('/panel/java').then((result) => result.runtimes);

export const createServer = (body: NewServer) => api.post<{ id: string }>('/servers', body);

export const uploadArchive = (file: File) =>
	api.post<{ upload_id: string; size_bytes: number }>('/servers/import/upload', file);

export const archiveRoots = (uploadId: string) =>
	api.get<ArchiveRoot[]>(`/servers/import/${uploadId}/entries`);
