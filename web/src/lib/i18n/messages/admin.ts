export const admin = {
	users: {
		title: 'Users',
		description: 'Who can sign in, and what each of them may do.',
		empty: 'Only you so far.',
		loadFailed: 'Could not read the users.',
		new: 'New user',
		edit: 'Edit {name}',
		columns: {
			name: 'Name',
			roles: 'Roles',
			lastLogin: 'Last signed in',
			status: 'Status'
		},
		badge: {
			administrator: 'Administrator',
			disabled: 'Disabled',
			never: 'Never'
		},
		field: {
			username: 'Username',
			password: 'Password',
			passwordKeep: 'Leave empty to keep the current one',
			passwordHint: 'At least 10 characters.',
			email: 'Email',
			admin: 'Administrator',
			adminHint: 'Every permission, on every server, always.',
			enabled: 'Can sign in',
			roles: 'Roles',
			rolesEmpty: 'No roles yet. Make one first, or grant administrator.'
		},
		keys: {
			title: 'API keys for {name}',
			description: 'For scripts and automation. A key can do whatever its owner can.',
			empty: 'No keys.',
			name: 'What is it for',
			create: 'Create key',
			created: 'Copy it now. This is the only time it is shown.',
			revoke: 'Revoke',
			lastUsed: 'Last used',
			never: 'Never used',
			manage: 'API keys'
		},
		done: {
			created: 'Added {name}',
			updated: 'Saved {name}',
			deleted: 'Deleted {name}'
		},
		confirmDelete: 'Delete {name}?',
		confirmDeleteBody: 'They lose access at once. This cannot be undone.'
	},
	roles: {
		title: 'Roles',
		description: 'A named set of permissions you can hand to somebody.',
		empty: 'No roles yet.',
		loadFailed: 'Could not read the roles.',
		new: 'New role',
		edit: 'Edit {name}',
		columns: {
			name: 'Name',
			members: 'People',
			permissions: 'Panel permissions',
			servers: 'Servers'
		},
		field: {
			name: 'Name',
			global: 'Panel permissions',
			globalHint: 'What holders may do outside any one server.',
			servers: 'Per-server permissions',
			serversHint:
				'Tick what holders may do on each server. Untouched servers stay invisible to them.',
			noServers: 'No servers to grant yet.'
		},
		counts: {
			none: 'None',
			servers_one: '1 server',
			servers_other: '{count} servers',
			members_one: '1 person',
			members_other: '{count} people'
		},
		done: {
			created: 'Added {name}',
			updated: 'Saved {name}',
			deleted: 'Deleted {name}'
		},
		confirmDelete: 'Delete {name}?',
		confirmDeleteBody: 'Anybody holding it loses what it granted. This cannot be undone.'
	},
	failed: 'That did not work'
} as const;
