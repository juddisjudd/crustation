import { admin } from './admin.ts';
import { common } from './common.ts';
import { create } from './create.ts';
import { dashboard } from './dashboard.ts';
import { files } from './files.ts';
import { nav } from './nav.ts';
import { players } from './players.ts';
import { server } from './server.ts';
import { settings } from './settings.ts';

export const en = {
	admin,
	common,
	create,
	dashboard,
	files,
	nav,
	players,
	server,
	settings
} as const;
