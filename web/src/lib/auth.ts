/**
 * Sign-in client.
 *
 * Posts the derived keystream (see `crypto.mjs`) — never the password — and
 * keeps the session in an HttpOnly cookie the API sets. Everything here is
 * browser-only: the pages that use it are prerendered and hydrate into it.
 */

import { deriveKeystream, toHex } from './crypto.mjs';

export interface SignedInUser {
	id: number;
	username: string;
}

export interface MyMatch {
	id: number;
	instance_id: number;
	map_name: string;
	map_label: string | null;
	game_mode: string;
	started_at: number;
	sealed_at: number | null;
	outcome: string | null;
	winning_task_force: number | null;
	task_force: number | null;
	queue_code: string | null;
	queue_label: string | null;
	queue_location: string | null;
	kills: number;
	deaths: number;
	assists: number;
	damage_dealt: number;
	healing: number;
	obj_points: number;
	time_played_seconds: number;
}

export interface CareerTotals {
	matches: number;
	kills: number;
	deaths: number;
	assists: number;
	damage_dealt: number;
	healing: number;
	obj_points: number;
	seconds_played: number;
}

export interface MmrEntry {
	class_name: string;
	mmr: number;
	games: number;
	played_at: number;
}

export interface MySummary {
	username: string;
	totals: CareerTotals;
	mmr: MmrEntry[];
}

/** Thrown for API failures, carrying the status and the server's message. */
export class ApiError extends Error {
	status: number;

	constructor(status: number, message: string) {
		super(message);
		this.status = status;
	}
}

async function readError(response: Response): Promise<string> {
	try {
		const body = (await response.json()) as { error?: string };
		return body.error ?? `request failed (${response.status})`;
	} catch {
		return `request failed (${response.status})`;
	}
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
	const response = await fetch(path, {
		credentials: 'same-origin',
		headers: { accept: 'application/json' },
		...init
	});
	if (!response.ok) throw new ApiError(response.status, await readError(response));
	return (await response.json()) as T;
}

/**
 * Sign in with in-game credentials.
 *
 * The derivation runs locally; only the resulting key material is sent.
 */
export async function signIn(username: string, password: string): Promise<SignedInUser> {
	const keystream = toHex(deriveKeystream(username, password));
	const body = await request<{ user: SignedInUser }>('/api/auth/login', {
		method: 'POST',
		headers: { 'content-type': 'application/json', accept: 'application/json' },
		body: JSON.stringify({ username, keystream })
	});
	return body.user;
}

export async function signOut(): Promise<void> {
	await request<{ ok: boolean }>('/api/auth/logout', { method: 'POST' });
}

export async function currentUser(): Promise<SignedInUser | null> {
	try {
		const body = await request<{ authenticated: boolean; user?: SignedInUser }>('/api/auth/me');
		return body.authenticated ? (body.user ?? null) : null;
	} catch {
		return null;
	}
}

export type MatchKindFilter = 'all' | 'pvp' | 'pve';

export interface MyMatchPage {
	kind: string;
	limit: number;
	page: number;
	total: number;
	has_more: boolean;
	matches: MyMatch[];
}

export function myMatches(
	kind: MatchKindFilter = 'all',
	page = 0,
	limit = 12
): Promise<MyMatchPage> {
	const params = new URLSearchParams({ kind, page: String(page), limit: String(limit) });
	return request<MyMatchPage>(`/api/me/matches?${params.toString()}`);
}

export function mySummary(): Promise<MySummary> {
	return request<MySummary>('/api/me/summary');
}
