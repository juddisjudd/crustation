import { api } from './client';

export interface FileEntry {
	name: string;
	kind: 'file' | 'directory';
	size: number;
	modified: string | null;
}

export interface Listing {
	path: string;
	entries: FileEntry[];
}

export interface FileContent {
	path: string;
	content: string;
	size: number;
	modified: string | null;
}

const base = (serverId: string) => `/servers/${serverId}/files`;

export const listFiles = (serverId: string, path: string) =>
	api.get<Listing>(base(serverId), { query: { path } });

export const readFile = (serverId: string, path: string) =>
	api.get<FileContent>(`${base(serverId)}/content`, { query: { path } });

export const saveFile = (
	serverId: string,
	path: string,
	content: string,
	modified: string | null
) =>
	api.put<{ size: number; modified: string | null }>(`${base(serverId)}/content`, {
		path,
		content,
		modified
	});

export const createEntry = (serverId: string, path: string, kind: 'file' | 'directory') =>
	api.post(base(serverId), { path, kind });

export const renameEntry = (serverId: string, path: string, newName: string) =>
	api.patch(base(serverId), { path, new_name: newName });

export const transferEntries = (
	serverId: string,
	paths: string[],
	destination: string,
	mode: 'copy' | 'move'
) => api.patch(base(serverId), { paths, destination, mode });

export const deleteEntries = (serverId: string, paths: string[]) =>
	api.delete(base(serverId), { paths });

export const extractArchive = (serverId: string, path: string) =>
	api.post(`${base(serverId)}/extract`, { path });

export const uploadFile = (serverId: string, path: string, file: File) =>
	api.post<{ size: number }>(`${base(serverId)}/upload`, file, { query: { path } });

/** A plain link, so the browser handles the save dialog and big files stream. */
export const downloadUrl = (serverId: string, path: string) =>
	`/api/v1${base(serverId)}/download?path=${encodeURIComponent(path)}`;

export const joinPath = (directory: string, name: string) =>
	directory ? `${directory}/${name}` : name;
