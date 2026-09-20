export const players = {
	title: 'Players',
	online: {
		title: 'On now',
		none: 'Nobody is on.',
		sampled:
			'Java sends a sample of the names, not the whole list, and none at all when the player list is hidden.',
		count: '{count} of {max}'
	},
	known: {
		title: 'Seen before',
		none: 'Nobody has been seen yet.',
		name: 'Name',
		first: 'First seen',
		last: 'Last seen',
		here: 'On now'
	},
	lists: {
		operators: 'Operators',
		allow: 'Allow list',
		banned: 'Banned players',
		'banned-ips': 'Banned addresses'
	},
	add: {
		name: 'Name',
		xuid: 'Xbox id',
		ip: 'Address',
		reason: 'Reason',
		level: 'Level',
		button: 'Add',
		added: 'Added {name}'
	},
	remove: 'Remove {name}',
	removed: 'Removed {name}',
	empty: 'Nothing on this list.',
	loadFailed: 'Could not read the player lists.',
	failed: 'That did not work'
} as const;
