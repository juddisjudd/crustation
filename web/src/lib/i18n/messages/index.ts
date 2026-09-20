import { admin } from './admin.ts';
import { addon } from './addon.ts';
import { chat } from './chat.ts';
import { common } from './common.ts';
import { content } from './content.ts';
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
	addon,
	chat,
	common,
	content,
	create,
	dashboard,
	files,
	map,
	nav,
	players,
	server,
	settings
} as const;
