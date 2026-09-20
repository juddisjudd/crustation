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
	other: {
		title: 'Everything else in the file',
		description: 'Keys the panel has no form for. Edit them as text.',
		managed: 'The panel writes this one.',
		empty: 'Nothing else is in the file.'
	},
	danger: {
		title: 'Delete this server',
		description:
			'The server is removed from the panel. Its stats, schedules and saved commands go with it. This cannot be undone.',
		files: 'Delete the files as well, including every world',
		filesHint:
			'Leave this off to keep the folder on disk and only remove the server from the panel.',
		button: 'Delete server',
		confirmTitle: 'Delete {name}?',
		confirmBody: 'This cannot be undone.',
		running: 'Stop the server before deleting it.',
		done: 'Deleted {name}',
		failed: 'Could not delete the server'
	}
} as const;
