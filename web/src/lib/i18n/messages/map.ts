export const map = {
	title: 'Map',
	servedOn: 'Served by the plugin on port {port}.',
	openTab: 'Open in a new tab',
	installedQuiet:
		'{name} is installed but nothing is listening on port {port} yet. Start the server, or give it a moment to finish rendering.',
	installedOff: '{name} is installed with its web server turned off. Turn it on in {config}.',
	radar: 'Where everybody is',
	north: 'N',
	nobody: 'Nobody is on.',
	stopped: 'The server is not running.',
	radarHint:
		'The panel asks the server where each player is over RCON, so this needs no plugin but draws no terrain. Install squaremap, BlueMap or Dynmap on the server and this tab shows the real rendered map instead.',
	dimension: {
		overworld: 'Overworld',
		the_nether: 'Nether',
		the_end: 'The End'
	}
} as const;
