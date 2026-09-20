import { common } from './common.ts';
import { create } from './create.ts';
import { dashboard } from './dashboard.ts';
import { nav } from './nav.ts';
import { server } from './server.ts';
import { settings } from './settings.ts';

export const en = { common, create, dashboard, nav, server, settings } as const;
