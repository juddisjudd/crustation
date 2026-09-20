export const create = {
	title: 'New server',
	description: 'Download a server, or import one you already have.',
	tabs: {
		install: 'Install',
		import: 'Import'
	},
	source: {
		title: 'What to run',
		installDescription: 'Crustation downloads the server and sets it up for you.',
		importDescription: 'Bring in a server that already exists.',
		version: 'Version',
		versionPlaceholder: 'Choose a version',
		loadingVersions: 'Loading versions…',
		versionsFailed: 'Could not load the versions for {provider}.',
		showUnstable: 'Show snapshots and pre-releases',
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
		uploadHint: 'A zip of the server folder.',
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
		resolving: 'Looking up the download',
		downloading: 'Downloading',
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
