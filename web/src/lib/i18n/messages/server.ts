export const server = {
	copyAddress: 'Copy address',
	installing: 'Installing',
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
		command: 'Start command'
	},
	console: {
		live: 'Live',
		offline: 'Offline',
		clear: 'Clear view',
		output: 'Server console output',
		waiting: 'Waiting for output…',
		stopped: 'The server is not running. Start it to see live output.',
		latest: 'Latest',
		command: 'Console command',
		placeholder: 'Type a command, for example: say Hello',
		placeholderStopped: 'Start the server to send commands',
		send: 'Send command',
		historyError: 'Could not load the console history',
		sendError: 'Command not sent'
	}
} as const;
