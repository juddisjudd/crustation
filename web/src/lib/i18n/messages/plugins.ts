export const plugins = {
	title: 'Plugins',
	framework: {
		title: 'Plugin framework',
		none: 'None',
		noneHint: 'Plain Rust, as Facepunch ships it.',
		oxide: 'Oxide',
		oxideHint:
			'The long-standing framework behind uMod. Steam updates remove it, so the panel puts it back after each one.',
		carbon: 'Carbon',
		carbonHint: 'Runs Oxide plugins too, leaves the game files alone, and updates itself.',
		current: 'Running {name}',
		apply: 'Apply',
		running: 'Stop the server before changing this.',
		working: 'Working…',
		started: 'Changing the plugin framework',
		startedHint: 'Follow along in the console.',
		failed: 'Could not change the plugin framework'
	},
	installed: {
		title: 'Installed',
		empty: 'No plugins yet.',
		upload: 'Upload',
		uploaded: 'Uploaded {name}',
		check: 'Check for updates',
		checking: 'Asking uMod…',
		upToDate: 'All up to date.',
		update: 'Update to {version}',
		updated: 'Updated {name}',
		enabled: 'On',
		remove: 'Remove {name}',
		removed: 'Removed {name}',
		failed: 'That did not work',
		hint: 'Each plugin is one .cs file the framework compiles as it lands. Its settings are in {folder}.'
	},
	umod: {
		title: 'uMod',
		search: 'Search uMod',
		empty: 'Nothing on uMod matches.',
		install: 'Install',
		installed: 'Installed',
		installedToast: 'Installed {name}',
		downloads: '{count} downloads',
		more: 'More results'
	}
} as const;
