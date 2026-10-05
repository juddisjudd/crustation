export const rustAdmin = {
	views: {
		convars: 'Convars',
		commands: 'Commands',
		items: 'Items'
	},
	generated: 'Facepunch wrote this description with a tool, so take it as a guide.',
	search: 'Search by name or description',
	clear: 'Clear',
	refresh: 'Refresh',
	failed: 'Could not load the list',
	showMore: 'Show {count} more',
	noMatch: 'Nothing matches.',
	convars: {
		count_one: '1 convar',
		count_other: '{count} convars',
		live: 'Live values from the running server.',
		starting:
			"The server is not answering yet, so this is Carbon's list. Live values appear once it is up.",
		stopped: 'The server is stopped. Each value is the start-up setting, or the default.',
		filters: {
			all: 'All',
			startup: 'On start-up',
			changed: 'Changed'
		},
		allGroups: 'All groups',
		onStartup: 'Start-up',
		managed: 'Panel',
		default: 'Default {value}',
		on: 'On',
		off: 'Off',
		undo: 'Undo',
		setNow: 'Set now',
		setNowHint: 'Tell the running server. It goes back at the next restart.',
		keep: 'Save',
		keepHint: 'Keep it on the start command, and set it now too if the server is running.',
		drop: 'Take {name} off the start command',
		setDone: '{name} is now {value} until the next restart.',
		keptNow: '{name} is now {value}, and kept for every start.',
		keptLater: '{name} is saved. It applies at the next start.',
		dropped: '{name} is off the start command. The next start uses the default.',
		failed: 'Could not change {name}'
	},
	commands: {
		count_one: '1 command',
		count_other: '{count} commands',
		common: 'Common',
		all: 'All',
		run: 'Run',
		send: 'Send',
		stopped: 'Start the server to run commands.',
		noReply: 'Done. Rust sent nothing back.',
		failed: 'Could not run that'
	},
	items: {
		count_one: '1 item',
		count_other: '{count} items',
		all: 'All',
		to: 'Give to',
		everyone: 'Everyone online',
		give: 'Give',
		noPlayers: 'Nobody is online, so only everyone online is offered.',
		stopped: 'Start the server to give items.',
		given: 'Gave {count} × {item} to {name}',
		givenAll: 'Gave {count} × {item} to everyone online',
		failed: 'Could not give that',
		copy: 'Copy short name',
		copied: 'Copied {name}'
	}
} as const;
