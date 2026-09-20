import { api } from './client';
import type { KnownProperty } from './create';

/** A catalogue entry with the value the file currently holds. */
export type ServerProperty = KnownProperty & {
	value: string;
	/** False when the file does not name it, so the server's own default applies. */
	set: boolean;
};

/** A key in the file the catalogue says nothing about. */
export interface OtherProperty {
	key: string;
	value: string;
	/** The panel writes this one itself, so it cannot be edited here. */
	managed: boolean;
}

export interface ServerProperties {
	exists: boolean;
	settings: ServerProperty[];
	other: OtherProperty[];
}

export const readProperties = (serverId: string) =>
	api.get<ServerProperties>(`/servers/${serverId}/properties`);

export const writeProperties = (serverId: string, settings: Record<string, string>) =>
	api.put<{ changed: string[]; restart_required: boolean }>(`/servers/${serverId}/properties`, {
		settings
	});
