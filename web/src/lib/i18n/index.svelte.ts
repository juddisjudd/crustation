import { api } from '$lib/api/client';
import { en } from './messages';

export { en };

type Join<K, P> = K extends string
	? P extends string
		? `${K}${'' extends P ? '' : '.'}${P}`
		: never
	: never;

type Leaves<T> = T extends string ? '' : { [K in keyof T]-?: Join<K, Leaves<T[K]>> }[keyof T];

export type MessageKey = Leaves<typeof en>;

type Vars = Record<string, string | number>;

function flatten(source: object, prefix = '', target: Record<string, string> = {}) {
	for (const [key, value] of Object.entries(source)) {
		const path = prefix ? `${prefix}.${key}` : key;
		if (typeof value === 'string') target[path] = value;
		else if (value && typeof value === 'object') flatten(value, path, target);
	}
	return target;
}

const fallback = flatten(en);

function interpolate(text: string, vars?: Vars) {
	if (!vars) return text;
	return text.replace(/\{(\w+)\}/g, (match, name) => (name in vars ? String(vars[name]) : match));
}

class I18n {
	/** Crafty language code, for example "de_DE". */
	locale = $state('en_EN');
	#messages = $state.raw<Record<string, string>>({});
	#loaded = new Map<string, Record<string, string>>();

	/** BCP 47 tag for Intl formatting. */
	get tag() {
		return this.locale.replace('_', '-');
	}

	async load(locale: string | null | undefined) {
		const code = locale || 'en_EN';
		if (code === this.locale && (code.startsWith('en') || Object.keys(this.#messages).length)) {
			return;
		}
		if (code.startsWith('en_') || code === 'en') {
			this.locale = code;
			this.#messages = {};
			return;
		}
		const cached = this.#loaded.get(code);
		if (cached) {
			this.locale = code;
			this.#messages = cached;
			return;
		}
		try {
			const result = await api.get<{ code: string; messages: Record<string, unknown> }>(
				`/ui/lang/${encodeURIComponent(code)}`,
				{ allowAnonymous: true }
			);
			const messages = flatten(result.messages ?? {});
			this.#loaded.set(code, messages);
			this.locale = code;
			this.#messages = messages;
		} catch {
			this.locale = code;
			this.#messages = {};
		}
	}

	t(key: MessageKey, vars?: Vars) {
		const text = this.#messages[key] ?? fallback[key];
		return text === undefined ? key : interpolate(text, vars);
	}

	/** Picks a key by count, e.g. plural('dashboard.serverCount', n) → …_one | …_other. */
	plural(key: string, count: number, vars?: Vars) {
		const rule = new Intl.PluralRules(this.tag).select(count);
		const exact = `${key}_${rule}` as MessageKey;
		const other = `${key}_other` as MessageKey;
		const text =
			this.#messages[exact] ?? fallback[exact] ?? this.#messages[other] ?? fallback[other];
		return text === undefined ? key : interpolate(text, { count, ...vars });
	}
}

export const i18n = new I18n();

/** Reactive inside components: re-runs when the language changes. */
export function t(key: MessageKey, vars?: Vars) {
	return i18n.t(key, vars);
}

export function plural(key: string, count: number, vars?: Vars) {
	return i18n.plural(key, count, vars);
}
