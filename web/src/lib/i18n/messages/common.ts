export const common = {
	actions: {
		save: 'Save',
		saving: 'Saving…',
		cancel: 'Cancel',
		close: 'Close',
		delete: 'Delete',
		edit: 'Edit',
		create: 'Create',
		reset: 'Reset',
		refresh: 'Refresh',
		retry: 'Try again',
		search: 'Search…',
		copy: 'Copy',
		copied: 'Copied',
		confirm: 'Confirm'
	},
	state: {
		loading: 'Loading…',
		none: 'None',
		unknown: 'Unknown',
		error: 'Something went wrong',
		yes: 'Yes',
		no: 'No'
	},
	server: {
		status: {
			running: 'Running',
			stopped: 'Stopped',
			starting: 'Starting',
			stopping: 'Stopping',
			crashed: 'Crashed',
			installing: 'Installing',
			unknown: 'Unavailable'
		},
		kind: {
			minecraft_java: 'Minecraft Java',
			minecraft_bedrock: 'Minecraft Bedrock'
		},
		action: {
			start: 'Start',
			stop: 'Stop',
			restart: 'Restart',
			kill: 'Kill process'
		}
	},
	permission: {
		COMMANDS: 'Commands',
		CONSOLE: 'Console',
		LOGS: 'Logs',
		SCHEDULES: 'Schedules',
		BACKUPS: 'Backups',
		FILES: 'Files',
		CONFIG: 'Settings',
		PLAYERS: 'Players',
		CREATE_SERVER: 'Create servers',
		MANAGE_USERS: 'Manage users',
		MANAGE_ROLES: 'Manage roles',
		ADMIN: 'Administrator'
	}
} as const;
