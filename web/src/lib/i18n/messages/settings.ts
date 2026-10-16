export const settings = {
	title: 'server.properties',
	description: 'Minecraft reads this file when it starts, so changes wait for the next start.',
	missing:
		'This server has no server.properties yet. It writes one the first time it runs; anything set here is written now and kept.',
	unset: 'Not set, so the server uses its own default.',
	changes_one: '1 change',
	changes_other: '{count} changes',
	saved_one: 'Saved 1 change',
	saved_other: 'Saved {count} changes',
	discard: 'Discard',
	save: 'Save',
	saving: 'Saving…',
	restart: 'Restart the server for this to take effect.',
	failed: 'Could not save',
	loadFailed: 'Could not read server.properties.',
	filter: 'Find a setting',
	clearFilter: 'Clear',
	noMatch: 'No setting matches "{query}".',
	edited: 'Not saved yet',
	nav: {
		label: 'Sections',
		other: 'Other'
	},
	other: {
		title: 'Everything else in the file',
		managed: 'The panel writes this one.',
		empty: 'Nothing else is in the file.'
	},
	rust: {
		title: 'Server settings',
		description:
			"Rust reads these when it starts, so changes wait for the next start. Clear a field to go back to the game's own default.",
		otherTitle: 'Other start-up settings',
		otherDescription: "Port settings follow the server's port.",
		otherEmpty: 'Nothing else is set.'
	},
	game: {
		enshrouded:
			"Enshrouded reads enshrouded_server.json when it starts and rewrites it while it runs, so change it while the server is stopped. Clear a field to go back to the game's own default.",
		dragonwilds:
			'Dragonwilds reads DedicatedServer.ini when it starts and rewrites it while it runs, so change it while the server is stopped. PvP and difficulty belong to the world, and the owner sets them in the game.',
		locked: 'Stop the server to change these. The game rewrites its settings file while it runs.',
		missing:
			'The game writes its settings file the first time it starts; anything set here is written now and kept.',
		otherTitle: 'Other keys in the file'
	},
	tags: {
		groups: {
			wipe: 'Wipe',
			difficulty: 'Difficulty',
			gameplay: 'Gameplay',
			modding: 'Mods',
			region: 'Region'
		},
		names: {
			monthly: 'Monthly',
			biweekly: 'Every two weeks',
			weekly: 'Weekly',
			vanilla: 'Vanilla',
			hardcore: 'Hardcore',
			softcore: 'Softcore',
			pve: 'PvE',
			roleplay: 'Roleplay',
			creative: 'Creative',
			minigame: 'Minigames',
			training: 'Training',
			battlefield: 'Battlefield',
			broyale: 'Battle royale',
			builds: 'Builds',
			tut: 'Tutorial',
			premium: 'Premium',
			modded: 'Modded',
			oxide: 'Oxide',
			carbon: 'Carbon'
		},
		other: 'Other',
		tooMany: 'The browser shows only four of these {count}.'
	},
	wipe: {
		title: 'Wipe now',
		description:
			'Deletes save files so the next start begins fresh. Settings, bans and staff stay.',
		map: 'Map',
		mapHint: 'The world and everything built in it.',
		blueprints: 'Blueprints',
		blueprintsHint: 'What players have learned.',
		full: 'Everything',
		fullHint: 'Map and blueprints, plus deaths, teams and clans.',
		newSeed: 'Roll a new seed',
		button: 'Wipe',
		confirmTitle: 'Wipe {name}?',
		confirmBody: 'This cannot be undone.',
		running: 'Stop the server before wiping it.',
		done_one: 'Deleted 1 file',
		done_other: 'Deleted {count} files',
		seed: 'The next map uses seed {seed}.',
		failed: 'Could not wipe the server'
	},
	danger: {
		title: 'Delete this server',
		description:
			'The server is removed from the panel. Its stats, schedules and saved commands go with it. This cannot be undone.',
		files: 'Delete the files as well, including every world',
		button: 'Delete server',
		confirmTitle: 'Delete {name}?',
		confirmBody: 'This cannot be undone.',
		running: 'Stop the server before deleting it.',
		done: 'Deleted {name}',
		failed: 'Could not delete the server'
	}
} as const;
