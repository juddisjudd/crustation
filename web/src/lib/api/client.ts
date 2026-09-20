import { base } from '$app/paths';

export const API_ROOT = '/api/v1';

export type ApiEnvelope<T> =
	{ status: 'ok'; data: T } | { status: 'error'; error: string; message?: string };

export class ApiError extends Error {
	constructor(
		readonly status: number,
		readonly code: string,
		message?: string,
		/** Whole error body: validation `fields`, `retry_after`, and anything else. */
		readonly payload?: Record<string, unknown>
	) {
		super(message || code);
		this.name = 'ApiError';
	}

	/** Field errors from a 422, keyed by field name. */
	get fields(): Record<string, string> {
		const fields = this.payload?.fields;
		return fields && typeof fields === 'object' ? (fields as Record<string, string>) : {};
	}

	get retryAfter(): number | undefined {
		const value = this.payload?.retry_after;
		return typeof value === 'number' ? value : undefined;
	}
}

type Query = Record<string, string | number | boolean | undefined | null>;

export interface RequestOptions extends Omit<RequestInit, 'body'> {
	body?: unknown;
	query?: Query;
	/** Do not redirect to the sign-in page when the session is missing. */
	allowAnonymous?: boolean;
}

function buildUrl(path: string, query?: Query) {
	const url = new URL(path.startsWith('/api') ? path : API_ROOT + path, location.origin);
	for (const [key, value] of Object.entries(query ?? {})) {
		if (value !== undefined && value !== null) url.searchParams.set(key, String(value));
	}
	return url.toString();
}

function redirectToLogin() {
	const next = location.pathname + location.search;
	if (!location.pathname.startsWith(`${base}/login`)) {
		location.href = `${base}/login?next=${encodeURIComponent(next)}`;
	}
}

export async function request<T = unknown>(path: string, options: RequestOptions = {}) {
	const { body, query, allowAnonymous, headers, ...init } = options;
	const isRaw = body instanceof FormData || body instanceof Blob || body instanceof ArrayBuffer;

	const response = await fetch(buildUrl(path, query), {
		credentials: 'same-origin',
		...init,
		headers: {
			Accept: 'application/json',
			...(body !== undefined && !isRaw ? { 'Content-Type': 'application/json' } : {}),
			...headers
		},
		body: body === undefined ? undefined : isRaw ? (body as BodyInit) : JSON.stringify(body)
	});

	const text = await response.text();
	let payload: (ApiEnvelope<T> & Record<string, unknown>) | undefined;
	try {
		payload = text ? JSON.parse(text) : undefined;
	} catch {
		payload = undefined;
	}

	if (!response.ok || payload?.status === 'error') {
		const code = (payload?.error as string) ?? `HTTP_${response.status}`;
		const message = (payload?.message as string) ?? text;
		if (response.status === 401 && !allowAnonymous) redirectToLogin();
		throw new ApiError(response.status, code, message, payload);
	}

	return (payload && 'data' in payload ? payload.data : undefined) as T;
}

export const api = {
	get: <T>(path: string, options?: RequestOptions) =>
		request<T>(path, { ...options, method: 'GET' }),
	post: <T>(path: string, body?: unknown, options?: RequestOptions) =>
		request<T>(path, { ...options, method: 'POST', body }),
	patch: <T>(path: string, body?: unknown, options?: RequestOptions) =>
		request<T>(path, { ...options, method: 'PATCH', body }),
	put: <T>(path: string, body?: unknown, options?: RequestOptions) =>
		request<T>(path, { ...options, method: 'PUT', body }),
	delete: <T>(path: string, body?: unknown, options?: RequestOptions) =>
		request<T>(path, { ...options, method: 'DELETE', body })
};
