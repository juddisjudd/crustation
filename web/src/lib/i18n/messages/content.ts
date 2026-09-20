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
	restartNeeded: 'Restart the server for this to take'
} as const;
