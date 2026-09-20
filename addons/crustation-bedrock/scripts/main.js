/*
 * Crustation's side of the bridge, running inside the Bedrock server.
 *
 * Bedrock has no RCON, so nothing outside the game can read chat or ask where
 * anybody is. This posts both to the panel on a short timer. It only ever
 * sends: commands already reach the server on its own standard input, so there
 * is nothing worth the risk of letting the panel push code in here.
 *
 * It needs @minecraft/server-net, which is only allowed on a Bedrock Dedicated
 * Server, only when the world has the Beta APIs experiment on, and only when
 * the module is listed in config/default/permissions.json. The panel writes
 * all three when it installs this.
 *
 * The event names move between API versions, so every subscription is made on
 * its own and a failure is reported rather than taking the rest down with it.
 * A pack that reports half a job is worth far more than one that dies on line
 * three and says nothing.
 */

import { ItemTypes, system, world } from '@minecraft/server';
import { http, HttpRequest, HttpRequestMethod, HttpHeader } from '@minecraft/server-net';
import { variables } from '@minecraft/server-admin';

/** Ticks between check-ins. Twenty ticks is one second at a healthy tick rate. */
const EVERY = 20;
/** Never let a backlog grow without bound if the panel is down. */
const MOST_HELD = 500;

const say = (text) => console.warn(`[Crustation] ${text}`);

say('script loaded');

const PANEL = readVariable('panelUrl');
const TOKEN = readVariable('token');

function readVariable(name) {
	try {
		const value = variables.get(name);
		return typeof value === 'string' && value.length > 0 ? value : null;
	} catch (error) {
		say(`cannot read ${name} from variables.json: ${error}`);
		return null;
	}
}

let held = [];
let talking = false;
let complained = false;
/** Set by the panel's reply. It only asks when it is holding no list at all. */
let itemsWanted = false;

function remember(event) {
	if (held.length >= MOST_HELD) held.shift();
	held.push(event);
}

/** Subscribes, and says which ones the game actually offered. */
function listen(what, attach) {
	try {
		attach();
		return what;
	} catch (error) {
		say(`no ${what} on this version: ${error}`);
		return null;
	}
}

const watching = [
	listen('chat', () =>
		world.afterEvents.chatSend.subscribe((event) => {
			remember({ kind: 'chat', player: event.sender.name, message: event.message });
		})
	),
	listen('join', () =>
		world.afterEvents.playerSpawn.subscribe((event) => {
			// Fired on respawn too, and only the first one is somebody arriving.
			if (event.initialSpawn) remember({ kind: 'join', player: event.player.name });
		})
	),
	listen('leave', () =>
		world.afterEvents.playerLeave.subscribe((event) => {
			remember({ kind: 'leave', player: event.playerName });
		})
	),
	listen('death', () =>
		world.afterEvents.entityDie.subscribe((event) => {
			const who = event.deadEntity;
			if (who?.typeId !== 'minecraft:player') return;
			remember({ kind: 'death', player: who.name, message: `${who.name} died` });
		})
	)
].filter(Boolean);

say(`watching: ${watching.join(', ') || 'nothing'}`);

/** Whether the panel's reply asked for the item list. A panel too old to say
 * is a panel that does not want one. */
function asked(body) {
	try {
		return JSON.parse(body)?.data?.want_items === true;
	} catch {
		return false;
	}
}

/**
 * Every item this server will take for `give`, add-ons included. Asking the
 * game beats any list the panel could ship: it is this version, these packs.
 */
function everyItem() {
	try {
		return ItemTypes.getAll().map((one) => one.id);
	} catch (error) {
		say(`cannot list the items: ${error}`);
		return null;
	}
}

async function checkIn() {
	if (talking) return;
	talking = true;

	// Taken before the request, so anything arriving mid-flight is kept for the
	// next one rather than lost with this one.
	const sending = held;
	held = [];

	try {
		const payload = { events: sending };
		if (itemsWanted) {
			const items = everyItem();
			if (items) payload.items = items;
		}

		const request = new HttpRequest(`${PANEL}/api/v1/bridge/${TOKEN}`);
		request.method = HttpRequestMethod.Post;
		request.headers = [new HttpHeader('Content-Type', 'application/json')];
		request.body = JSON.stringify(payload);
		request.timeout = 10;

		const reply = await http.request(request);
		if (reply.status < 200 || reply.status >= 300) {
			throw new Error(`panel answered ${reply.status}`);
		}
		if (payload.items) say(`sent ${payload.items.length} item ids`);
		itemsWanted = asked(reply.body);
		if (complained) say('panel reachable again');
		complained = false;
	} catch (error) {
		// Put them back at the front: they have not been delivered.
		held = sending.concat(held).slice(-MOST_HELD);
		if (!complained) {
			complained = true;
			say(`cannot reach the panel: ${error}`);
		}
	} finally {
		talking = false;
	}
}

if (!PANEL || !TOKEN) {
	say('no panelUrl or token in variables.json, so nothing is being sent. Install from the panel.');
} else {
	say(`talking to ${PANEL} every ${EVERY} ticks`);
	system.runInterval(() => {
		checkIn();
	}, EVERY);
}
