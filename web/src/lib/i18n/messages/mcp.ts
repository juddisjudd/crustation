export const mcp = {
	title: 'MCP server',
	hint: 'Lets an AI assistant read your servers and act on them, through the Model Context Protocol.',

	enabled: 'Serving',
	disabled: 'Switched off',
	toggle: 'Answer requests at /mcp',
	toggleHint:
		'Takes effect at once. While it is off the endpoint answers 503 and nothing can reach it.',
	turnedOn: 'MCP is now serving',
	turnedOff: 'MCP is switched off',
	failed: 'That did not work',

	connect: 'Connecting a client',
	connectHint:
		'Run this where the assistant is. The key goes in the header; it is never part of the URL.',
	endpoint: 'Endpoint',
	copy: 'Copy',
	copied: 'Copied',

	keyTitle: 'It needs an API key',
	keyBody:
		'A session will not do: a browser sends its cookie to any site that asks, and this endpoint is reachable from one. Make a key on the Users page, under the three dots beside a person.',
	keyAction: 'Go to Users',
	keyScope:
		'A key with no permissions of its own can do whatever its owner can. Give the key its own, narrower set if the assistant should reach less than you do.',

	tools: 'Tools',
	toolsHint:
		'Every call is checked the same way the web interface would be. A tool that needs a permission the key does not hold answers that it is not allowed.',
	columnTool: 'Tool',
	columnNeeds: 'Needs',
	needsNothing: 'Any key',

	resources: 'Resources',
	resourcesHint: 'For a client that would rather attach state than call a tool.',

	configNote:
		'The config file sets what this is on a fresh start ({value}). The switch above overrides it and is remembered.',
	configOn: 'on',
	configOff: 'off'
} as const;
