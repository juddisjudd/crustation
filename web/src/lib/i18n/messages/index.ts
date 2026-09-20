import { admin } from './admin.ts';
import { chat } from './chat.ts';
import { common } from './common.ts';
import { create } from './create.ts';
import { dashboard } from './dashboard.ts';
import { files } from './files.ts';
import { map } from './map.ts';
import { nav } from './nav.ts';
import { players } from './players.ts';
import { server } from './server.ts';
import { settings } from './settings.ts';

export const en = {
	admin,
	chat,
	common,
	create,
	dashboard,
	files,
	map,
	nav,
	players,
	server,
	settings
} as const;
