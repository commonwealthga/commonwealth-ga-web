/**
 * The player's own settings — the toggles that used to be chat-command only.
 *
 * Reading is free (the values live in the game database). Writing goes through
 * this site to the control server, because the live effect has to be pushed to
 * the game client; that is why a write can fail while a read succeeds.
 */

export interface Preference {
	key: string;
	value: string;
	enabled: boolean;
}

export interface DlcPack {
	identifier: string;
	name: string;
	installed: boolean;
	/** `pvp` or `pve`, derived from the pack's own maps. */
	mode: 'pvp' | 'pve' | null;
}

export interface PlayerSettings {
	preferences: Preference[];
	dlc: DlcPack[];
	/** Whether this deployment can deliver changes at all. */
	writable: boolean;
}

async function send<T>(method: string, url: string, body?: unknown): Promise<T> {
	const response = await fetch(url, {
		method,
		credentials: 'same-origin',
		headers: { accept: 'application/json', 'content-type': 'application/json' },
		body: body === undefined ? undefined : JSON.stringify(body)
	});

	if (!response.ok) {
		// The status carries the meaning here: 503 means the game server
		// endpoint is missing or down, 400 means the change was refused.
		if (response.status === 503) throw new Error('The game server is not reachable right now.');
		if (response.status === 400) throw new Error('The game server refused that change.');
		if (response.status === 401) throw new Error('Sign in again to change settings.');
		throw new Error(`unavailable (${response.status})`);
	}
	return (await response.json()) as T;
}

export const fetchSettings = () => send<PlayerSettings>('GET', '/api/me/settings');

export const setPreference = (key: string, value: string) =>
	send<{ ok: boolean; applied_live: boolean }>('POST', '/api/me/settings', { key, value });

export const setDlcInstalled = (identifier: string, installed: boolean) =>
	send<{ ok: boolean; applied_live: boolean }>('POST', '/api/me/settings/dlc', {
		identifier,
		installed
	});
