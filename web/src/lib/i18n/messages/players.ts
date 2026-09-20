export const players = {
	title: 'Players',
	people: 'People',
	listsTitle: 'Lists',
	online: {
		title: 'On now',
		none: 'Nobody has been on this server yet.',
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
	badge: {
		operator: 'OP',
		operatorHint: 'On the operator list.',
		banned: 'Banned'
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
	act: {
		menu: 'What to do about {name}',
		op: 'Make an operator',
		opped: '{name} is an operator',
		deop: 'Take operator away',
		deopped: '{name} is no longer an operator',
		rank: 'Set rank',
		ranked: 'Set {name} to {rank}',
		level: 'Level {level}',
		ranks: {
			visitor: 'Visitor',
			member: 'Member',
			operator: 'Operator'
		},
		kick: 'Kick',
		kickTitle: 'Kick {name}',
		kicked: 'Kicked {name}',
		ban: 'Ban',
		banTitle: 'Ban {name}',
		banned: 'Banned {name}',
		pardon: 'Lift the ban',
		pardoned: 'Let {name} back in',
		reasonHint: 'They see this. Leave it empty for the usual wording.',
		reasonPlaceholder: 'Why?',
		give: 'Give an item',
		giveTitle: 'Give something to {name}',
		giveHint: 'Any id the server takes. The list is only the common ones.',
		gave: 'Gave {item} to {name}',
		item: 'Item',
		count: 'How many',
		teleport: 'Teleport',
		teleportTitle: 'Send {name} somewhere',
		teleportHint: 'To somebody else, or to a spot in the world.',
		teleported: 'Sent {name} to {to}',
		toPlayer: 'To a player',
		toPlace: 'To a place',
		go: 'Send',
		whisper: 'Send a message',
		whisperTitle: 'Message {name}',
		message: 'Message',
		send: 'Send',
		sent: 'Sent',
		say: 'Say',
		sayPlaceholder: 'Say something to everyone on the server…',
		onRestart: 'It takes effect the next time the server starts.',
		viaFile: 'Written to the file, since the server is down.'
	},
	fileHint: 'This table is {file}, beside the world.',
	remove: 'Remove {name}',
	removed: 'Removed {name}',
	empty: 'Nothing on this list.',
	loadFailed: 'Could not read the player lists.',
	failed: 'That did not work'
} as const;
