export const schedules = {
	title: 'Schedules',
	new: 'New schedule',
	edit: 'Edit {name}',
	empty: 'Nothing scheduled.',
	columns: {
		name: 'Name',
		when: 'When',
		what: 'What',
		next: 'Next run',
		last: 'Last run'
	},
	never: 'Never',
	waiting: 'Waits for the one before',
	field: {
		name: 'Name',
		when: 'When',
		what: 'What',
		every: 'Every',
		cron: 'Cron line',
		cronHint:
			'Minute, hour, day, month and weekday, in the panel’s time. Both day fields have to match: 0 19 1-7 * 4 is the first Thursday at 7pm.',
		after: 'After',
		delay: 'Minutes later',
		command: 'Command',
		warn: 'Warn players',
		kind: 'What goes',
		newSeed: 'Roll a new seed',
		onlyIfNewer: 'Only when Steam has a newer build',
		oneTime: 'Run once, then switch off'
	},
	units: {
		minutes: 'minutes',
		hours: 'hours',
		days: 'days'
	},
	triggers: {
		interval: 'Repeat',
		cron: 'At set times',
		after: 'After another schedule'
	},
	actions: {
		start: 'Start',
		stop: 'Stop',
		restart: 'Restart',
		command: 'Run a command',
		wipe: 'Wipe',
		update: 'Update from Steam'
	},
	describe: {
		every: 'Every {count} {unit}',
		everyOne: {
			minutes: 'Every minute',
			hours: 'Every hour',
			days: 'Every day'
		},
		after: '{minutes} min after {name}',
		wipe: 'Wipe: {kind}',
		command: 'Run {command}'
	},
	presets: {
		title: 'Start from',
		nightly: 'Restart every night at 4am',
		weekly: 'Map wipe every Thursday at 7pm',
		monthly: 'Full wipe on the first Thursday at 8pm',
		updates: 'Update when Steam has a new build'
	},
	runNow: 'Run now',
	ran: 'Running {name}',
	saved: 'Saved {name}',
	deleted: 'Deleted {name}',
	confirmDelete: 'Delete {name}?',
	failed: 'That did not work'
} as const;
