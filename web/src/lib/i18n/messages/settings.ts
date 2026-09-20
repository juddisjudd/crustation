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
	}
} as const;
