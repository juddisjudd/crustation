import { api } from './client';

export interface McpTool {
	name: string;
	description: string | null;
	/** The per-server permission the call asks for, or null when it needs none. */
	permission: string | null;
}

export interface McpStatus {
	enabled: boolean;
	/** What the config file says a fresh start would do. */
	config_default: boolean;
	/** Set when the panel knows its own external address. */
	public_url: string | null;
	tools: McpTool[];
	resources: string[];
}

export const mcpStatus = () => api.get<McpStatus>('/panel/mcp');

export const setMcp = (enabled: boolean) =>
	api.patch<{ enabled: boolean }>('/panel/mcp', { enabled });
