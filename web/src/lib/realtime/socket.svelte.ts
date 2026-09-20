type Listener = (data: any) => void;

const HEARTBEAT_MS = 25_000;

/**
 * One socket for the whole app. Views subscribe to topics as they mount, so
 * navigating never reconnects.
 */
class CrustationSocket {
	status = $state<'idle' | 'connecting' | 'open' | 'closed'>('idle');

	#ws: WebSocket | null = null;
	#listeners = new Map<string, Set<Listener>>();
	#topics = new Set<string>();
	#failures = 0;
	#retryTimer: ReturnType<typeof setTimeout> | undefined;
	#heartbeat: ReturnType<typeof setInterval> | undefined;
	#stopped = true;

	connect() {
		this.#stopped = false;
		if (this.#ws && this.#ws.readyState <= WebSocket.OPEN) return;
		clearTimeout(this.#retryTimer);

		const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
		const ws = new WebSocket(`${protocol}//${location.host}/ws`);
		this.#ws = ws;
		this.status = 'connecting';

		ws.onopen = () => {
			this.status = 'open';
			this.#failures = 0;
			if (this.#topics.size) {
				this.#send({ type: 'subscribe', topics: [...this.#topics] });
			}
			clearInterval(this.#heartbeat);
			this.#heartbeat = setInterval(() => this.#send({ type: 'ping' }), HEARTBEAT_MS);
		};
		ws.onmessage = (raw) => {
			let frame: { type: string; topic?: string; event?: string; data?: unknown };
			try {
				frame = JSON.parse(raw.data);
			} catch {
				return;
			}
			if (frame.type !== 'event' || !frame.topic || !frame.event) return;
			this.#listeners.get(`${frame.topic}/${frame.event}`)?.forEach((fn) => fn(frame.data));
			this.#listeners.get(`*/${frame.event}`)?.forEach((fn) => fn(frame.data));
		};
		ws.onclose = () => {
			clearInterval(this.#heartbeat);
			if (this.#ws !== ws) return;
			this.#ws = null;
			this.status = 'closed';
			if (!this.#stopped) this.#scheduleReconnect();
		};
	}

	disconnect() {
		this.#stopped = true;
		clearTimeout(this.#retryTimer);
		clearInterval(this.#heartbeat);
		const ws = this.#ws;
		this.#ws = null;
		this.#topics.clear();
		ws?.close(1000, 'Client closed');
		this.status = 'idle';
	}

	/** Subscribes while the caller lives; returns the unsubscribe. */
	subscribe(topics: string[]) {
		const added = topics.filter((topic) => !this.#topics.has(topic));
		topics.forEach((topic) => this.#topics.add(topic));
		if (added.length) this.#send({ type: 'subscribe', topics: added });
		return () => {
			topics.forEach((topic) => this.#topics.delete(topic));
			this.#send({ type: 'unsubscribe', topics });
		};
	}

	/** Listens for one event on one topic; `*` matches any topic. */
	on<T = any>(topic: string, event: string, listener: (data: T) => void) {
		const key = `${topic}/${event}`;
		let set = this.#listeners.get(key);
		if (!set) this.#listeners.set(key, (set = new Set()));
		set.add(listener as Listener);
		return () => set.delete(listener as Listener);
	}

	#send(message: unknown) {
		if (this.#ws?.readyState === WebSocket.OPEN) {
			this.#ws.send(JSON.stringify(message));
		}
	}

	#scheduleReconnect() {
		const delay = Math.min(30_000, 1_000 * 2 ** this.#failures) + Math.random() * 1_000;
		this.#failures++;
		this.#retryTimer = setTimeout(() => this.connect(), delay);
	}

	handleVisibility() {
		if (document.visibilityState === 'visible' && !this.#stopped && !this.#ws) {
			this.#failures = 0;
			this.connect();
		}
	}
}

export const socket = new CrustationSocket();

/** Subscribes to topics for the lifetime of the calling effect. */
export function useTopics(topics: () => string[]) {
	$effect(() => socket.subscribe(topics()));
}
