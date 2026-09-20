import type { ConsoleLine } from '$lib/api/types';

export type ChatKind = 'chat' | 'server' | 'action' | 'join' | 'leave' | 'advancement';

export interface ChatEvent {
	seq: number;
	at: string;
	kind: ChatKind;
	/** The player it is about, when there is one. */
	who?: string;
	/** Raw, so section-sign colours survive into the view. */
	text: string;
}

/**
 * Drops the log opening so what is left is what the server actually said.
 * Java writes `[12:34:56] [Server thread/INFO]: …` and Bedrock writes
 * `[2026-09-20 00:00:00:000 INFO] …`.
 */
function body(text: string) {
	const java = /^\[\d{1,2}:\d{2}:\d{2}\]\s*\[[^\]]+\]:\s?/.exec(text);
	if (java) return text.slice(java[0].length);
	const bedrock = /^\[\d{4}-\d{2}-\d{2}[^\]]*\]\s?/.exec(text);
	if (bedrock) return text.slice(bedrock[0].length);
	return text;
}

const CHAT = /^<([^>\s]{1,32})>\s?([\s\S]*)$/;
const SERVER = /^\[Server\]\s?([\s\S]*)$/;
const ACTION = /^\* (\S{1,32}) ([\s\S]+)$/;
const MOVED = /^(\w{1,16}) (joined|left) the game$/;
// Bedrock says it its own way, and is the only thing it says about chat at all.
const BEDROCK_MOVED = /^Player (connected|disconnected):\s*([^,]+)/;
const EARNED =
	/^(\w{1,16}) has (?:made the advancement|completed the challenge|reached the goal) (.+)$/;

/**
 * Picks the chat out of a console line, or nothing when the line is not chat.
 *
 * Bedrock's dedicated server does not log what players type at all, so only
 * the coming and going shows up there. That is the server's limit, not ours.
 */
export function chatOf(line: ConsoleLine): ChatEvent | null {
	if (line.stream !== 'stdout') return null;
	// 1.19 and later mark unsigned messages; that marker is not the message.
	const rest = body(line.text).replace(/^\[Not Secure\]\s*/, '');
	const at = line.at;
	const seq = line.seq;

	let found = CHAT.exec(rest);
	if (found) return { seq, at, kind: 'chat', who: found[1], text: found[2] };

	found = SERVER.exec(rest);
	if (found) return { seq, at, kind: 'server', text: found[1] };

	found = ACTION.exec(rest);
	if (found) return { seq, at, kind: 'action', who: found[1], text: found[2] };

	found = MOVED.exec(rest);
	if (found) {
		return { seq, at, kind: found[2] === 'joined' ? 'join' : 'leave', who: found[1], text: '' };
	}

	found = BEDROCK_MOVED.exec(rest);
	if (found) {
		return {
			seq,
			at,
			kind: found[1] === 'connected' ? 'join' : 'leave',
			who: found[2].trim(),
			text: ''
		};
	}

	found = EARNED.exec(rest);
	if (found) return { seq, at, kind: 'advancement', who: found[1], text: found[2] };

	return null;
}
