export const content = {
	title: 'Add-ons and worlds',
	hint: 'Install a pack or bring in a world. Both take effect the next time the server starts.',

	installAddon: 'Install add-on',
	importWorld: 'Import world',
	installing: 'Unpacking…',
	addonTypes: 'A .mcaddon, .mcpack or zip holding one.',
	worldTypes: 'A .mcworld, or a zip of a world folder.',

	packs: 'Installed add-ons',
	noPacks: 'Nothing installed yet.',
	noPacksHint: 'Install an add-on and it will show up here, with the world it is switched on for.',
	showStock: 'Show the ones the server came with',
	stockHidden: '{count} hidden that Bedrock ships with itself',
	stock: 'Shipped',
	onlyStock: 'Only the packs the server came with.',
	onlyStockHint: 'Nothing has been added to this server yet.',
	worlds: 'Worlds',
	noWorlds: 'No worlds yet.',
	noWorldsHint: 'Start the server once and it will make one, or import a world here.',

	active: 'Loaded',
	inactive: 'Not loaded',
	playing: 'Being played',
	playThis: 'Play this world',
	folder: 'Folder',
	version: 'Version {version}',

	sortBehaviour: 'Behaviour pack',
	sortResource: 'Resource pack',
	sortDatapack: 'Datapack',
	sortSkin: 'Skin pack',
	sortWorldTemplate: 'World template',
	sortWorld: 'World',

	chooseWorld: 'Which world?',
	chooseWorldBody:
		'This server keeps more than one world. The add-on is switched on for the one you pick; the others are left alone.',
	chooseWorldConfirm: 'Install here',
	cancel: 'Cancel',

	useWorld: 'Play this world once it is in',
	useWorldHint: 'Points level-name at it. Leave it off to import the world without switching.',

	installed: 'Installed {names}',
	installedWhere: 'It loads the next time the server starts.',
	installedFor: 'Switched on for {world}. It loads the next time the server starts.',
	notSwitchedOn:
		'The files are in place, but {world} could not be told to load them. Check its world_behavior_packs.json and world_resource_packs.json, then install again.',
	worldImported: 'Imported {names}',
	nowPlaying: 'The server now plays {world}. Restart it to load.',
	worldKept: 'It is in the worlds folder. Point level-name at it to play it.',
	failed: 'That did not work',
	restartNeeded: 'Restart the server for this to take',

	missingTitle: 'This world names packs it does not have',
	missingBody:
		'The server reports each of these once at startup as "Configured pack was not found and was ignored", then carries on without it. Either the pack was removed while the world still named it, or two packs shared a folder. Removing the entry stops the warning; it does not touch any pack that is installed.',
	missingId: 'Pack id',
	forget: 'Remove entry',
	forgotten: 'Removed {uuid} from {world}',
	packId: 'Pack id {uuid}',

	remove: 'Remove',
	removeTitle: 'Remove {name}?',
	removeBody:
		'Its files are deleted and it is taken out of every world that loads it. Anything it added to a world stays in that world; the game just stops knowing what it was.',
	removeStock: 'This is one the server came with. Removing it may stop the server loading.',
	removed: 'Removed {name}',
	removedFrom: 'Taken out of {worlds}.',
	removedNowhere: 'No world was loading it.',
	notes: 'Notes',
	notesFor: 'Notes on {name}',
	notesBody:
		'For what this add-on needs run in the game: how to switch it on, what puts it right when it misbehaves.',
	notesPlaceholder: 'Sneak and break an ore to vein-mine. Tools below diamond are ignored.',
	notesSave: 'Save',
	notesSaved: 'Saved',
	notesEmpty: 'Nothing written down yet.',
	notesHas: 'Has a note',
	notesCommands: 'Buttons',
	notesCommandsBody: 'Each one runs on the server, the same as typing it into the console.',
	notesAddCommand: 'Add a button',
	notesLabel: 'Button',
	notesCommand: 'Command',
	notesRemoveCommand: 'Remove {name}',
	notesRun: 'Run',
	notesRan: 'Sent {name}',
	notesPlayerWarning:
		'These run as the server, not as a player. A command aimed at @s or @p wants typing in the game instead.',
	notesFound: 'Found in this pack',
	notesFoundBody: 'Read out of the pack itself, so it is a starting point rather than a promise.',
	notesAdd: 'Add',
	notesKindFunction: 'Functions',
	notesKindScriptevent: 'Script events',
	notesKindCommand: 'Slash commands',
	notesKindSetting: 'World settings',
	notesKindSettingBody: 'Changed in the game, from the world settings, not from a console.',
	notesNothingFound: 'Nothing in this pack looks like a command.',

	removedSkipped:
		'{worlds} still names it: that pack list could not be read, so it was left alone. Fix it by hand, or the server will warn about a pack it cannot find.'
} as const;
