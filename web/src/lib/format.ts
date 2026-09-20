import { i18n, t, type MessageKey } from '$lib/i18n/index.svelte';

const MC_CODES = /§[0-9a-fk-or]/gi;

/** Minecraft MOTDs carry colour codes; the interface shows the words only. */
export function stripMotd(value: string | null | undefined) {
	return (value || '').replace(MC_CODES, '').trim();
}

export function percent(value: number | null | undefined, digits = 0) {
	if (value === null || value === undefined || Number.isNaN(Number(value))) return '—';
	return `${Number(value).toFixed(digits)}%`;
}

export function clampPercent(value: number | null | undefined) {
	const n = Number(value);
	return Number.isFinite(n) ? Math.min(100, Math.max(0, n)) : 0;
}

export function bytes(value: number | null | undefined) {
	const n = Number(value);
	if (!Number.isFinite(n) || n <= 0) return '0 B';
	const units = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];
	let size = n;
	let unit = 0;
	while (size >= 1024 && unit < units.length - 1) {
		size /= 1024;
		unit++;
	}
	return `${size.toFixed(size >= 10 || unit === 0 ? 0 : 1)} ${units[unit]}`;
}

/** The panel sends RFC 3339 in UTC. */
export function parseTime(value: string | null | undefined) {
	if (!value) return null;
	const date = new Date(value);
	return Number.isNaN(date.getTime()) ? null : date;
}

export function dateTime(value: string | Date | null | undefined) {
	const date = value instanceof Date ? value : parseTime(value);
	return date ? date.toLocaleString(i18n.tag) : '—';
}

export function duration(ms: number) {
	const seconds = Math.max(0, Math.floor(ms / 1000));
	const days = Math.floor(seconds / 86400);
	const hours = Math.floor((seconds % 86400) / 3600);
	const minutes = Math.floor((seconds % 3600) / 60);
	if (days) return `${days}d ${hours}h`;
	if (hours) return `${hours}h ${minutes}m`;
	if (minutes) return `${minutes}m`;
	return `${seconds}s`;
}

export function relativeTime(date: Date | null) {
	if (!date) return '—';
	const rtf = new Intl.RelativeTimeFormat(i18n.tag, { numeric: 'auto' });
	const diff = (date.getTime() - Date.now()) / 1000;
	const units: [Intl.RelativeTimeFormatUnit, number][] = [
		['year', 31536000],
		['month', 2592000],
		['day', 86400],
		['hour', 3600],
		['minute', 60],
		['second', 1]
	];
	for (const [unit, size] of units) {
		if (Math.abs(diff) >= size || unit === 'second') {
			return rtf.format(Math.round(diff / size), unit);
		}
	}
	return '—';
}

const KNOWN_KINDS = ['minecraft_java', 'minecraft_bedrock'];

export function kindLabel(kind: string) {
	return KNOWN_KINDS.includes(kind) ? t(`common.server.kind.${kind}` as MessageKey) : kind;
}
