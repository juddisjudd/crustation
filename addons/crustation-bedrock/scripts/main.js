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
 */

import { system, world } from '@minecraft/server';
import { http, HttpRequest, HttpRequestMethod, HttpHeader } from '@minecraft/server-net';
import { variables } from '@minecraft/server-admin';

/** Ticks between check-ins. Twenty ticks is one second at a healthy tick rate. */
const EVERY = 20;
/** Never let a backlog grow without bound if the panel is down. */
const MOST_HELD = 500;

const PANEL = readVariable('panelUrl');
const TOKEN = readVariable('token');

function readVariable(name) {
	try {
		const value = variables.get(name);
		return typeof value === 'string' && value.length > 0 ? value : null;
	} catch {
		return null;
	}
}

let held = [];
let talking = false;
let complained = false;

function remember(event) {
	if (held.length >= MOST_HELD) held.shift();
	held.push(event);
}

world.afterEvents.chatSend.subscribe((event) => {
	remember({ kind: 'chat', player: event.sender.name, message: event.message });
});

world.afterEvents.playerSpawn.subscribe((event) => {
	// Fired on respawn too, and only the first one is somebody arriving.
	if (event.initialSpawn) remember({ kind: 'join', player: event.player.name });
});

world.beforeEvents.playerLeave.subscribe((event) => {
	remember({ kind: 'leave', player: event.player.name });
});

world.afterEvents.entityDie.subscribe((event) => {
	const who = event.deadEntity;
	if (who?.typeId !== 'minecraft:player') return;
	remember({ kind: 'death', player: who.name, message: `${who.name} died` });
});

/** Where everybody is, which is the whole point of the map. */
function positions() {
	const out = [];
	for (const player of world.getAllPlayers()) {
		const at = player.location;
		out.push({
			name: player.name,
			x: Math.round(at.x * 100) / 100,
			y: Math.round(at.y * 100) / 100,
			z: Math.round(at.z * 100) / 100,
			dimension: player.dimension.id
		});
	}
	return out;
}

async function checkIn() {
	if (talking) return;
	talking = true;

	// Taken before the request, so anything arriving mid-flight is kept for the
	// next one rather than lost with this one.
	const sending = held;
	held = [];

	try {
		const request = new HttpRequest(`${PANEL}/api/v1/bridge/${TOKEN}`);
		request.method = HttpRequestMethod.Post;
		request.headers = [new HttpHeader('Content-Type', 'application/json')];
		request.body = JSON.stringify({ events: sending, players: positions() });
		request.timeout = 10;

		const reply = await http.request(request);
		if (reply.status < 200 || reply.status >= 300) {
			throw new Error(`panel answered ${reply.status}`);
		}
		complained = false;
	} catch (error) {
		// Put them back at the front: they have not been delivered.
		held = sending.concat(held).slice(-MOST_HELD);
		if (!complained) {
			complained = true;
			console.warn(`[Crustation] cannot reach the panel: ${error}`);
		}
	} finally {
		talking = false;
	}
}

if (!PANEL || !TOKEN) {
	console.warn(
		'[Crustation] no panelUrl or token in variables.json, so the add-on is doing nothing. ' +
			'Install it from the panel rather than by hand.'
	);
} else {
	console.warn(`[Crustation] talking to ${PANEL}`);
	system.runInterval(() => {
		checkIn();
	}, EVERY);
}
