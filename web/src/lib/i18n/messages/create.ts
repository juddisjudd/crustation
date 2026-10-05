export const create = {
	title: 'New server',
	game: {
		title: 'Game'
	},
	type: {
		title: 'Server type',
		java: 'Java Edition',
		bedrock: 'Bedrock Edition',
		other: 'Already have one',
		import: 'Import a server'
	},
	tabs: {
		install: 'Install',
		import: 'Import'
	},
	source: {
		title: 'What to run',
		branch: 'Branch',
		version: 'Version',
		versionPlaceholder: 'Choose a version',
		loadingVersions: 'Loading versions…',
		versionsFailed: 'Could not load the versions for {provider}.',
		showUnstable: 'Show snapshots and pre-releases',
		showUnstableRust: 'Show staging and test branches',
		noVersions: 'No versions to choose from.'
	},
	import: {
		mode: 'Import from',
		modeZip: 'A zip you upload',
		modeFolder: 'A folder on this host',
		modeUrl: 'A download link',
		kind: 'Server type',
		file: 'Archive',
		uploading: 'Uploading…',
		uploaded: 'Uploaded {name}',
		root: 'Folder inside the archive',
		rootTop: 'The top level',
		rootHint: 'The folder holding server.properties.',
		rootsEmpty: 'Nothing in this archive looks like a server. Import it anyway, or pick another.',
		path: 'Folder path',
		pathHint: 'An absolute path on the machine running the panel. It is copied, not moved.',
		pathAdminOnly: 'Only an administrator can import a folder from this host.',
		url: 'Download link',
		urlHint: 'A .zip is unpacked; anything else is kept as the server jar.',
		executable: 'Start file',
		executableHint: 'Leave this empty and Crustation works it out.'
	},
	details: {
		title: 'Details',
		name: 'Name',
		namePlaceholder: 'Survival',
		port: 'Port',
		host: 'Bind address',
		hostHint: '0.0.0.0 answers on every network on this host.',
		minMemory: 'Least memory (MB)',
		maxMemory: 'Most memory (MB)',
		java: 'Java',
		javaAuto: 'Newest the version accepts',
		javaNone: 'No Java found on this host. The server will not start until there is one.',
		flags: 'JVM flags',
		flagsHint:
			'Left empty, the server runs on the flags its project recommends, sized to the memory limit.',
		advanced: 'Advanced',
		autostart: 'Start this server when the panel starts'
	},
	properties: {
		title: 'Server settings',
		show: 'Change the world and player settings',
		failed: 'Could not load the settings for this kind of server.',
		random: 'Random'
	},
	eula: {
		label: 'I accept the Minecraft end user licence agreement',
		link: 'Read it'
	},
	submit: 'Create server',
	submitting: 'Creating…',
	failed: 'Could not create the server',
	uploadFailed: 'Could not upload the archive',
	progress: {
		title: 'Setting up {name}',
		waiting: 'Starting…',
		steamcmd: 'Setting up SteamCMD',
		signing_in: 'Signing in to Steam',
		resolving: 'Looking up the download',
		preallocating: 'Making room on disk',
		downloading: 'Downloading',
		verifying: 'Checking the files',
		committing: 'Moving the files into place',
		extracting: 'Unpacking',
		copying: 'Copying files',
		installing: 'Running the installer',
		configuring: 'Writing settings',
		done: 'Ready to start',
		failed: 'Install failed',
		open: 'Open the server',
		again: 'Create another',
		leaveHint: 'This carries on if you navigate away.'
	}
} as const;
