export const server = {
	copyAddress: 'Copy address',
	installing: 'Installing',
	updating: 'Updating',
	health: {
		title: 'Server health',
		framerate: 'Frame rate',
		fps: '{fps} fps',
		entities: 'Entities',
		queue: 'Queue',
		queueValue: '{queued} waiting, {joining} joining',
		network: 'Network',
		networkValue: '{in}/s in, {out}/s out',
		rustPlus: 'Rust+',
		rustPlusStates: {
			off: 'Switched off',
			failed: 'Not reachable',
			listening: 'Listening',
			unknown: 'Starting up'
		},
		rustPlusWhere: 'The app connects to {address}.',
		rustPlusFix:
			'Publish TCP {port} and forward it on your router, or set the Rust+ public address in Settings.'
	},
	network: {
		title: 'Network',
		local: 'Local address',
		public: 'Public address',
		localUnknown: 'Not known from here',
		localHint: 'Open the panel by its IP address and it shows here.',
		publicUnknown: 'Could not be found',
		ports: 'Ports',
		refresh: 'Check again',
		checking: 'Checking…',
		whenRunning: 'The ports are checked while the server is running.',
		failed: 'Could not read the network details.',
		names: {
			game: 'Game',
			query: 'Query',
			rust_plus: 'Rust+',
			beacon: 'World settings beacon'
		},
		states: {
			open: 'Open',
			closed: 'Closed',
			quiet: 'Not listening',
			unanswered: 'No answer',
			listening: 'Listening',
			unknown: 'Unknown'
		},
		untestable:
			'This port answers no question the panel can ask, so it was not tried from outside.',
		hints: {
			open: 'Asked from the panel through {address}, the way a player outside reaches it.',
			closed:
				'The closed ports did not answer through {address}. Forward them on your router to this machine.',
			closedDocker:
				'The closed ports did not answer through {address}. Publish them on the container and forward them on your router.',
			unanswered:
				'Nothing answered through {address}. Either the ports are not forwarded, or your router does not send traffic for its own address back in. The panel cannot tell which from here.',
			untested: 'Only checked on this machine, because the public address is not known.'
		}
	},
	map: {
		title: 'Map',
		alt: 'The map this server is running',
		open: 'Open the full picture'
	},
	updates: {
		title: 'Updates',
		installed: 'Installed build',
		latest: 'Newest on {branch}',
		upToDate: 'Up to date',
		available: 'Update available',
		unknown: 'Steam did not answer',
		checking: 'Asking Steam…',
		button: 'Update now',
		warn: 'Warn players',
		warnings: {
			none: 'No warning',
			m1: '1 minute',
			m5: '5 minutes',
			m15: '15 minutes'
		},
		started: 'Updating {name}',
		startedRunning:
			'Players get a countdown in chat, then the server stops, updates and starts again.',
		startedSilent: 'The server stops now, updates and starts again.',
		failed: 'Could not start the update'
	},
	actions: 'Server actions',
	openConsole: 'Open console',
	noActions: 'No actions available',
	killTitle: 'Kill {name}?',
	killBody:
		'The process ends immediately without saving. Use this only when the server does not respond to Stop.',
	killConfirm: 'Kill process',
	overview: {
		uptime: 'Uptime',
		startedAt: 'Started {when}',
		liveUsage: 'Live usage',
		cpuUsage: 'CPU usage',
		memoryUsage: 'Memory usage',
		slotsUsed: 'Player slots used',
		details: 'Details',
		motd: 'Message of the day',
		version: 'Version',
		type: 'Type',
		port: 'Port',
		autoStart: 'Start with the panel',
		autoStartOn: 'Yes, after {seconds}s',
		memoryLimit: 'Memory',
		folder: 'Folder',
		latency: 'Ping',
		command: 'Start command',
		activity: {
			title: 'Last hour',
			label: '{name} over the last hour',
			noData: 'Nothing recorded yet.'
		},
		rust: {
			queryPort: 'Query port',
			name: 'Server name',
			branch: 'Branch',
			map: 'Map',
			seed: 'Seed',
			seedRandom: 'Random',
			mapSize: 'Map size',
			gameMode: 'Game mode',
			standard: 'standard',
			rules: 'Rules',
			pve: 'PvE',
			pvp: 'PvP',
			tags: 'Tags',
			framework: 'Plugin framework',
			frameworks: {
				oxide: 'Oxide',
				carbon: 'Carbon',
				none: 'None'
			},
			connectTitle: 'Connect',
			connectHint: 'In Rust, press F1 and enter this. The server has to be running.'
		},
		game: {
			listName: 'Name in the list',
			slots: 'Player slots',
			preset: 'Preset',
			textChat: 'Text chat',
			voiceChat: 'Voice chat',
			beaconPort: 'Beacon port',
			worldName: 'World name',
			platforms: 'Who can join',
			password: 'World password',
			owner: 'Owner',
			ownerSet: 'Set',
			ownerMissing: 'Missing, so the server will not start',
			connectEnshrouded:
				"Find {name} in the game's server list, or add this address as a Steam favourite. Players join with a role's password.",
			connectDragonwilds:
				'In the game, open Worlds, then Public, and search for {world}. Case matters. Players on the same network use Direct with this address.'
		}
	},
	motd: {
		title: 'Message of the day',
		preview: 'How it will look',
		empty: 'Nothing set yet.',
		saved: 'Message of the day saved',
		onRestart: 'It takes effect the next time the server starts.',
		failed: 'Could not save that',
		strip: 'Take the colours out',
		overlong: 'Longer than 59 characters, so the game will cut it off.',
		tooTall: 'Only the first {lines} line(s) will show.'
	},
	portClash: 'Port {port} is also used by {names}. Only one of them can start.',
	console: {
		live: 'Live',
		offline: 'Offline',
		clear: 'Clear the buffer',
		clearError: 'Could not clear the console',
		macros: {
			add: 'Save a command',
			label: 'Button label',
			command: 'Command',
			remove: 'Remove {name}',
			saved: 'Saved {name}',
			hint: 'Saved commands appear as buttons for everybody who can use this console.'
		},
		syntax: 'Colour the log',
		timestamps: 'Show timestamps',
		download: 'Download the buffer',
		search: 'Search',
		regex: 'Match as a regular expression',
		badRegex: 'That is not a valid regular expression.',
		noMatch: 'Nothing here matches.',
		filtered: 'Showing {shown} of {total}',
		level: {
			error: 'Error',
			warn: 'Warning',
			info: 'Info',
			other: 'Other',
			noise: 'Noise',
			steamcmd: 'SteamCMD'
		},
		quiet: {
			noise: 'What the game prints on every start and nobody needs to read.',
			steamcmd: 'What SteamCMD prints while it works. Its result and its errors always show.'
		},
		output: 'Server console output',
		waiting: 'Waiting for output…',
		stopped: 'The server is not running.',
		latest: 'Latest',
		command: 'Console command',
		placeholder: 'Type a command, for example: say Hello',
		placeholderStopped: 'Start the server to send commands',
		readOnly: 'This game reads no commands, so its console is for reading only.',
		send: 'Send command',
		historyError: 'Could not load the console history',
		sendError: 'Command not sent',
		rconOn: 'RCON',
		rconOnHint: 'Commands run over RCON, so the server answers back here.',
		rconOff: 'Enable RCON',
		rconOffHint:
			'Turn on RCON so commands return the server’s answer. It writes a password into server.properties and takes effect on the next restart.',
		rconEnabledRestart: 'RCON is on. Restart the server to start using it.',
		rconEnabledNow: 'RCON is on, listening on port {port}.',
		rconError: 'Could not turn RCON on'
	}
} as const;
