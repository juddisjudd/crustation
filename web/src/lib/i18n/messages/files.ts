export const files = {
	root: 'Server folder',
	empty: 'This folder is empty.',
	loadFailed: 'Could not read this folder.',
	columns: {
		name: 'Name',
		size: 'Size',
		modified: 'Changed'
	},
	actions: {
		newFile: 'New file',
		newFolder: 'New folder',
		upload: 'Upload',
		uploading: 'Uploading {name}',
		download: 'Download',
		rename: 'Rename',
		unpack: 'Unpack here',
		install: 'Install as an add-on',
		delete: 'Delete',
		more: 'More for {name}'
	},
	prompt: {
		newFile: 'Name for the new file',
		newFolder: 'Name for the new folder',
		rename: 'New name for {name}'
	},
	editor: {
		save: 'Save',
		saving: 'Saving…',
		back: 'Back',
		close: 'Close',
		discard: 'Discard them',
		discardTitle: 'Leave without saving?',
		discardBody: 'Your changes to this file are lost.',
		saved: 'Saved {name}',
		unsaved: 'Unsaved changes',
		tooBig: 'Too big or not text. Download it instead.'
	},
	confirm: {
		deleteTitle: 'Delete {name}?',
		deleteBody: 'This cannot be undone.',
		deleteFolder: 'Everything inside it goes too. This cannot be undone.',
		unpackTitle: 'Unpack {name}?',
		unpackBody: 'Files already there with the same names are overwritten.'
	},
	done: {
		created: 'Created {name}',
		renamed: 'Renamed to {name}',
		deleted: 'Deleted {name}',
		unpacked: 'Unpacked {name}',
		uploaded: 'Uploaded {name}',
		installed: 'Installed {names}',
		installedWhere:
			'Put where the server reads it, and switched on for the world. It loads on the next start.'
	},
	failed: 'That did not work'
} as const;
