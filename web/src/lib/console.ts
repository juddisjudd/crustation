import type { ConsoleLine } from '$lib/api/types';

export type Level = 'error' | 'warn' | 'info' | 'other';

/** Minecraft's section-sign palette, as both editions write it. */
const COLOURS: Record<string, string> = {
	'0': '#000000',
	'1': '#0000aa',
	'2': '#00aa00',
	'3': '#00aaaa',
	'4': '#aa0000',
	'5': '#aa00aa',
	'6': '#ffaa00',
	'7': '#aaaaaa',
	'8': '#555555',
	'9': '#5555ff',
	a: '#55ff55',
	b: '#55ffff',
	c: '#ff5555',
	d: '#ff55ff',
	e: '#ffff55',
	f: '#ffffff'
};

const LEVEL = /\b(FATAL|SEVERE|ERROR|WARN(?:ING)?|INFO|DEBUG|TRACE)\b/;

/**
 * Reads the level out of a line. Both editions put it near the front, Java as
 * `[Server thread/INFO]` and Bedrock as `[2026-09-20 00:00:00:000 INFO]`, so
 * only the opening is searched and a later "ERROR" in a message is ignored.
 */
export function levelOf(line: ConsoleLine): Level {
	if (line.stream === 'stderr') return 'error';
	const found = LEVEL.exec(line.text.slice(0, 80))?.[1];
	switch (found) {
		case 'FATAL':
		case 'SEVERE':
		case 'ERROR':
			return 'error';
		case 'WARN':
		case 'WARNING':
			return 'warn';
		case 'INFO':
			return 'info';
		default:
			return 'other';
	}
}

export interface Piece {
	text: string;
	colour?: string;
	bold?: boolean;
	italic?: boolean;
	underline?: boolean;
	strike?: boolean;
	/** A Tailwind class for the kind of thing this run is. */
	tone?: string;
	/** Part of what the search matched. */
	hit?: boolean;
}

/** Drops the colour codes, for searching and for the downloaded copy. */
export function plain(text: string) {
	return text.replace(/§[0-9a-fk-or]/gi, '');
}

/**
 * Splits a line into styled runs. Colour codes reset the styles that came
 * before, the way the game reads them, and `§r` clears everything.
 */
export function pieces(text: string, matcher: RegExp | null, syntax = false): Piece[] {
	const runs: Piece[] = [];
	let current: Piece = { text: '' };

	const push = () => {
		if (current.text) runs.push({ ...current });
	};

	for (let index = 0; index < text.length; index++) {
		const character = text[index];
		if (character !== '§' || index === text.length - 1) {
			current.text += character;
			continue;
		}

		const code = text[++index].toLowerCase();
		push();
		current = { ...current, text: '' };

		if (code === 'r') current = { text: '' };
		else if (COLOURS[code]) current = { text: '', colour: COLOURS[code] };
		else if (code === 'l') current.bold = true;
		else if (code === 'o') current.italic = true;
		else if (code === 'n') current.underline = true;
		else if (code === 'm') current.strike = true;
		// 'k' is the obfuscated scramble, which the panel shows as plain text.
	}
	push();

	const marked = syntax ? runs.flatMap(markup) : runs;
	return matcher ? marked.flatMap((run) => split(run, matcher)) : marked;
}

/** Breaks one run apart wherever the search matched inside it. */
function split(run: Piece, matcher: RegExp): Piece[] {
	const out: Piece[] = [];
	const pattern = new RegExp(
		matcher.source,
		matcher.flags.includes('g') ? matcher.flags : `${matcher.flags}g`
	);
	let last = 0;

	for (const match of run.text.matchAll(pattern)) {
		if (match.index === undefined || match[0] === '') continue;
		if (match.index > last) out.push({ ...run, text: run.text.slice(last, match.index) });
		out.push({ ...run, text: match[0], hit: true });
		last = match.index + match[0].length;
	}
	if (last === 0) return [run];
	if (last < run.text.length) out.push({ ...run, text: run.text.slice(last) });
	return out;
}

/**
 * Builds the search pattern. Deliberately not global: `test` on a global regex
 * carries `lastIndex` between calls, so filtering a list with one would skip
 * every other line. `split` adds the flag where it needs it.
 */
export function matcherFor(query: string, asRegex: boolean): RegExp | null {
	const trimmed = query.trim();
	if (!trimmed) return null;
	if (!asRegex) return new RegExp(escape(trimmed), 'i');
	try {
		return new RegExp(trimmed, 'i');
	} catch {
		return null;
	}
}

/** True when the text the operator typed is not a usable pattern. */
export function badRegex(query: string, asRegex: boolean) {
	if (!asRegex || !query.trim()) return false;
	try {
		new RegExp(query, 'gi');
		return false;
	} catch {
		return true;
	}
}

function escape(value: string) {
	return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/** The buffer as a plain text file, timestamps first. */
export function asText(lines: ConsoleLine[]) {
	return lines.map((line) => `[${new Date(line.at).toISOString()}] ${plain(line.text)}`).join('\n');
}

/**
 * The shape of a log line, so the eye can skip past the parts that repeat on
 * every line and land on the message. Only runs the game did not colour itself
 * are marked up; a `§` colour always wins, since the server meant that one.
 */
const GRAMMAR = new RegExp(
	[
		// Java's clock, and Bedrock's whole date-and-level opening.
		String.raw`(?<time>\[\d{1,2}:\d{2}:\d{2}(?:[.:]\d{1,3})?\])`,
		String.raw`(?<stamp>\[\d{4}-\d{2}-\d{2}[^\]]{0,40}\])`,
		// [Server thread/INFO], [main/WARN] and the like.
		String.raw`(?<thread>\[[\w #.\-]{1,40}\/(?:FATAL|SEVERE|ERROR|WARN(?:ING)?|INFO|DEBUG|TRACE)\])`,
		String.raw`(?<url>\bhttps?:\/\/\S+)`,
		String.raw`(?<uuid>\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\b)`,
		String.raw`(?<address>\b\d{1,3}(?:\.\d{1,3}){3}(?::\d{1,5})?\b)`,
		// <Notch> in chat, and the /command somebody ran.
		String.raw`(?<speaker><[^<>\s]{1,32}>)`,
		String.raw`(?<quoted>"[^"\n]{0,200}")`,
		String.raw`(?<frame>\bat [\w$.]+\([^)\n]{0,120}\))`,
		String.raw`(?<number>-?\b\d+(?:\.\d+)?\b)`
	].join('|'),
	'g'
);

/** Tailwind classes for each kind of token, against the console's own black. */
export const TONES: Record<string, string> = {
	time: 'text-white/30',
	stamp: 'text-white/30',
	thread: 'text-white/35',
	url: 'text-[#8ab4f8] underline',
	uuid: 'text-white/35',
	address: 'text-[#c4a7f7]',
	speaker: 'text-[#7cc4f5]',
	quoted: 'text-[#9ad6a4]',
	frame: 'text-white/35',
	number: 'text-[#d8b070]'
};

/** Splits one uncoloured run into the parts the grammar recognises. */
function markup(run: Piece): Piece[] {
	if (run.colour) return [run];
	const out: Piece[] = [];
	let last = 0;

	GRAMMAR.lastIndex = 0;
	for (const match of run.text.matchAll(GRAMMAR)) {
		if (match.index === undefined) continue;
		const kind = Object.entries(match.groups ?? {}).find(([, held]) => held !== undefined)?.[0];
		if (!kind) continue;
		if (match.index > last) out.push({ ...run, text: run.text.slice(last, match.index) });
		out.push({ ...run, text: match[0], tone: TONES[kind] });
		last = match.index + match[0].length;
	}

	if (last === 0) return [run];
	if (last < run.text.length) out.push({ ...run, text: run.text.slice(last) });
	return out;
}
