export const addon = {
	title: 'Crustation add-on',
	hint: 'Bedrock has no RCON. This lets the panel read chat and see where people are.',
	connected: 'Connected',
	waiting: 'Installed, waiting',
	notInstalled: 'Not installed',
	lastSeen: 'Last heard from {when}',
	url: 'Where the server should reach the panel',
	urlHint:
		'The game server calls this address, not your browser. Loopback is right when both run in the same container.',
	install: 'Install the add-on',
	reinstall: 'Install again',
	installed: 'Add-on installed',
	onRestart: 'It starts talking the next time the server starts.',
	betaTurnedOn: 'Beta APIs turned on for the world, which the add-on needs. Restart to load it.',
	noWorldYet:
		'There is no world yet, so nothing was switched on for it. Start the server once, then install again.',
	remove: 'Remove',
	removeTitle: 'Remove the add-on?',
	removeBody: 'Chat and the live map stop working for this server. Nothing else is touched.',
	removed: 'Add-on removed',
	quiet:
		'Installed, but the server has not called in. Restart it, and check the address above is one it can reach.',
	betaOff:
		'The world does not have Beta APIs on, so the game will not give the add-on the network module. Install again to turn it on.',
	noWorld: 'There is no world at {world} yet. Start the server once, then install again.',
	failed: 'That did not work'
} as const;
