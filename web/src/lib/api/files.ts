import { api, ApiError } from './client';

export interface FilePermissions {
	can_read: boolean;
	can_write: boolean;
	can_execute: boolean;
}

export interface FileEntry {
	name: string;
	path: string;
	dir: boolean;
	canOpen: boolean;
	mime: string | null;
	/** Server-local "YYYY/MM/DD HH:MM". Sorts correctly as a string. */
	modified: string;
	size: string;
	bytes: number;
	permissions: FilePermissions;
}

export interface Listing {
	path: string;
	modified: number;
	top: boolean;
	entries: FileEntry[];
}

export interface FileContent {
	content: string;
	mime: string | null;
	modified: string;
	size: string;
	modifiedEpoch: number;
}

interface RawEntry {
	path: string;
	dir: boolean;
	can_open?: boolean;
	mime?: string | null;
	modified?: string;
	size?: string;
	permissions?: FilePermissions;
}

interface RawRoot {
	local_path: string;
	path: string;
	top: boolean;
	modified: number;
}

const DEFAULT_PERMISSIONS: FilePermissions = {
	can_read: false,
	can_write: false,
	can_execute: false
};

export function normalizePath(path: string | null | undefined) {
	return (path ?? '')
		.replace(/\\/g, '/')
		.replace(/\/+/g, '/')
		.replace(/^\/+|\/+$/g, '');
}

export function joinPath(dir: string, name: string) {
	const base = normalizePath(dir);
	return base ? `${base}/${name}` : name;
}

export function parentPath(path: string) {
	const value = normalizePath(path);
	const index = value.lastIndexOf('/');
	return index === -1 ? '' : value.slice(0, index);
}

export function baseName(path: string) {
	const value = normalizePath(path);
	return value.slice(value.lastIndexOf('/') + 1);
}

export function pathSegments(path: string) {
	const value = normalizePath(path);
	return value ? value.split('/') : [];
}

export function fileExtension(path: string) {
	const name = baseName(path);
	const index = name.lastIndexOf('.');
	return index <= 0 ? '' : name.slice(index + 1).toLowerCase();
}

const SIZE_UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB', 'EB', 'ZB', 'YB'];

/** Crafty returns sizes as "1.1KB". Parse them back so columns can be sorted. */
export function parseSize(size: string | undefined) {
	const match = /^\s*([\d.]+)\s*([KMGTPEZY]?)i?B\s*$/i.exec(size ?? '');
	if (!match) return 0;
	const unit = SIZE_UNITS.indexOf(`${match[2].toUpperCase()}B`);
	return Number(match[1]) * 1024 ** (unit === -1 ? 0 : unit);
}

export function formatSize(bytes: number) {
	let value = bytes;
	let unit = 0;
	while (value >= 1024 && unit < SIZE_UNITS.length - 1) {
		value /= 1024;
		unit++;
	}
	return `${value.toFixed(value >= 10 || unit === 0 ? 0 : 1)} ${SIZE_UNITS[unit]}`;
}

const dateFormat = new Intl.DateTimeFormat(undefined, {
	day: 'numeric',
	month: 'short',
	hour: '2-digit',
	minute: '2-digit'
});

/** Crafty sends "YYYY/MM/DD HH:MM" in the host's local time. */
export function formatModified(modified: string) {
	const parts = /^(\d{4})\/(\d{2})\/(\d{2}) (\d{2}):(\d{2})$/.exec(modified ?? '');
	if (!parts) return modified || '—';
	const [, year, month, day, hour, minute] = parts;
	const date = new Date(+year, +month - 1, +day, +hour, +minute);
	const label = dateFormat.format(date);
	return date.getFullYear() === new Date().getFullYear() ? label : `${label}, ${year}`;
}

export function isConflict(err: unknown) {
	return err instanceof ApiError && err.status === 409;
}

/** Returns `null` when the server answers 304, meaning the listing is unchanged. */
export async function listDirectory(
	serverId: string,
	path: string,
	modifiedEpoch?: number
): Promise<Listing | null> {
	const body: Record<string, unknown> = { page: 'files', path: normalizePath(path) };
	if (modifiedEpoch) body.modified_epoch = modifiedEpoch;

	let raw: Record<string, RawEntry | RawRoot>;
	try {
		raw = await api.post<Record<string, RawEntry | RawRoot>>(`/servers/${serverId}/files`, body);
	} catch (err) {
		if (err instanceof ApiError && err.status === 304) return null;
		throw err;
	}

	const root = raw.root_path as RawRoot | undefined;
	const entries: FileEntry[] = [];
	for (const [name, value] of Object.entries(raw)) {
		if (name === 'root_path') continue;
		const entry = value as RawEntry;
		entries.push({
			name,
			path: normalizePath(entry.path),
			dir: !!entry.dir,
			canOpen: !!entry.can_open,
			mime: entry.mime ?? null,
			modified: entry.modified ?? '',
			size: entry.size ?? '',
			bytes: entry.dir ? 0 : parseSize(entry.size),
			permissions: entry.permissions ?? DEFAULT_PERMISSIONS
		});
	}

	return {
		path: normalizePath(root?.local_path ?? path),
		modified: root?.modified ?? 0,
		top: !!root?.top,
		entries
	};
}

export async function readFile(serverId: string, path: string): Promise<FileContent> {
	const data = await api.post<{
		content: string;
		attributes: { mime: string | null; modified: string; size: string; modified_epoch: number };
	}>(`/servers/${serverId}/files`, { page: 'files', path: normalizePath(path) });
	return {
		content: data.content,
		mime: data.attributes.mime,
		modified: data.attributes.modified,
		size: data.attributes.size,
		modifiedEpoch: data.attributes.modified_epoch
	};
}

export async function saveFile(
	serverId: string,
	path: string,
	contents: string,
	modifiedEpoch: number,
	overwrite = false
) {
	const data = await api.patch<{
		attributes: { mime: string | null; modified: string; size: string; modified_epoch: number };
	}>(`/servers/${serverId}/files`, {
		path: normalizePath(path),
		contents,
		modified_epoch: modifiedEpoch,
		...(overwrite ? { overwrite: true } : {})
	});
	return data.attributes;
}

export function createItem(serverId: string, parent: string, name: string, directory: boolean) {
	return api.put(`/servers/${serverId}/files/create`, {
		parent: normalizePath(parent),
		name,
		directory
	});
}

export function renameItem(serverId: string, path: string, newName: string) {
	return api.patch(`/servers/${serverId}/files/create`, {
		path: normalizePath(path),
		new_name: newName
	});
}

export function deleteItems(serverId: string, paths: string[]) {
	return api.delete(`/servers/${serverId}/files`, {
		file_system_objects: paths.map((path) => ({ filename: normalizePath(path) }))
	});
}

export function transferItems(
	serverId: string,
	operation: 'copy' | 'move',
	paths: string[],
	target: string
) {
	return api.post(`/servers/${serverId}/files/${operation}`, {
		file_system_objects: paths.map((path) => ({
			source_path: normalizePath(path),
			target_path: normalizePath(target)
		}))
	});
}

export function unzipFile(serverId: string, path: string, procId: string) {
	return api.post(`/servers/${serverId}/files/zip`, {
		folder: normalizePath(path),
		proc_id: procId
	});
}

export function downloadUrl(serverId: string, path: string) {
	return `/api/v2/servers/${serverId}/files/${encodeURIComponent(normalizePath(path))}/download`;
}
