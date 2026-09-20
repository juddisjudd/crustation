import { api } from './client';

export interface Macro {
	id: string;
	label: string;
	command: string;
	position: number;
}

const base = (serverId: string) => `/servers/${serverId}/macros`;

export const listMacros = (serverId: string) => api.get<Macro[]>(base(serverId));

export const createMacro = (serverId: string, label: string, command: string) =>
	api.post<{ id: string }>(base(serverId), { label, command });

export const deleteMacro = (serverId: string, id: string) => api.delete(`${base(serverId)}/${id}`);
