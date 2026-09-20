export const chat = {
	title: 'Chat',
	quiet: 'Nothing said yet.',
	stopped: 'The server is not running.',
	placeholder: 'Say something as the server…',
	placeholderStopped: 'Start the server to talk',
	send: 'Send',
	asServer: 'Server',
	joined: '{name} joined',
	left: '{name} left',
	earned: '{name} earned {what}',
	failed: 'Could not send that',
	bedrock:
		'Bedrock servers do not write chat to the console, so only who comes and goes shows here. Messages you send still reach everyone in game.'
} as const;
