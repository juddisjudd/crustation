import { api } from './client';
import type { GlobalPermission, ServerPermission } from './types';

export interface UserSummary {
	id: string;
	username: string;
	email: string | null;
	language: string;
	is_admin: boolean;
	enabled: boolean;
	created_at: string;
	last_login_at: string | null;
	roles: { id: string; name: string }[];
}

export interface NewUser {
	username: string;
	password: string;
	email?: string | null;
	is_admin?: boolean;
	enabled?: boolean;
	roles?: string[];
}

export interface UserChanges {
	username?: string;
	/** Left out to keep the one they have. */
	password?: string;
	email?: string | null;
	is_admin?: boolean;
	enabled?: boolean;
	roles?: string[];
}

export interface ServerGrant {
	server_id: string;
	permissions: ServerPermission[];
}

export interface Role {
	id: string;
	name: string;
	created_at: string;
	members: number;
	global_permissions: GlobalPermission[];
	servers: ServerGrant[];
}

export interface RoleChanges {
	name?: string;
	global_permissions?: GlobalPermission[];
	servers?: ServerGrant[];
}

export interface ApiKey {
	id: string;
	name: string;
	created_at: string;
	last_used_at: string | null;
}

export const listUsers = () => api.get<UserSummary[]>('/users');
export const createUser = (body: NewUser) => api.post<{ id: string }>('/users', body);
export const updateUser = (id: string, body: UserChanges) =>
	api.patch<UserSummary>(`/users/${id}`, body);
export const deleteUser = (id: string) => api.delete(`/users/${id}`);

export const listRoles = () => api.get<Role[]>('/roles');
export const createRole = (body: RoleChanges) => api.post<{ id: string }>('/roles', body);
export const updateRole = (id: string, body: RoleChanges) => api.patch<Role>(`/roles/${id}`, body);
export const deleteRole = (id: string) => api.delete(`/roles/${id}`);

export const listKeys = (userId: string) => api.get<ApiKey[]>(`/users/${userId}/api-keys`);
/** The token comes back once, here, and is never readable again. */
export const mintKey = (userId: string, name: string) =>
	api.post<{ id: string; name: string; token: string }>(`/users/${userId}/api-keys`, { name });
export const revokeKey = (userId: string, keyId: string) =>
	api.delete(`/users/${userId}/api-keys/${keyId}`);

export const GLOBAL_PERMISSIONS: GlobalPermission[] = [
	'CREATE_SERVER',
	'MANAGE_USERS',
	'MANAGE_ROLES',
	'ADMIN'
];

export const SERVER_PERMISSIONS: ServerPermission[] = [
	'COMMANDS',
	'CONSOLE',
	'LOGS',
	'SCHEDULES',
	'BACKUPS',
	'FILES',
	'CONFIG',
	'PLAYERS'
];
