/**
 * Public agent data: the directory, one agent's profile, rating history and
 * the class leaderboards.
 *
 * Everything here is read-only and needs no session — these are the pages
 * players use to look each other up.
 */

import type { MatchSummary } from '$lib/live';

export interface AgentRow {
	user_id: number;
	username: string;
	matches: number;
	last_active: number | null;
	damage_dealt: number;
}

export interface DirectoryPage {
	query: string | null;
	limit: number;
	page: number;
	total: number;
	has_more: boolean;
	agents: AgentRow[];
}

export interface RankedRating {
	class_name: string;
	mmr: number;
	games: number;
	played_at: number;
	rank: number;
	ranked_players: number;
}

export interface ModeTotals {
	matches: number;
	wins: number;
	kills: number;
	deaths: number;
	assists: number;
	damage_dealt: number;
	damage_taken: number;
	healing: number;
	obj_points: number;
	seconds_played: number;
	bot_kills: number;
	beacons_destroyed: number;
}

/** Career totals split by what was being played. */
export interface CareerSplit {
	all: ModeTotals;
	pvp: ModeTotals;
	pve: ModeTotals;
}

/** The player's best single match at one statistic. */
export interface BestStat {
	stat: string;
	value: number;
	instance_id: number;
	map_label: string | null;
	map_name: string;
	queue_code: string | null;
	started_at: number;
}

export interface BestSplit {
	all: BestStat[];
	pvp: BestStat[];
	pve: BestStat[];
}

export type PlayMode = 'all' | 'pvp' | 'pve';

export interface AgentProfile {
	user_id: number;
	username: string;
	first_seen: number | null;
	last_active: number | null;
	totals: CareerSplit;
	bests: BestSplit;
	ratings: RankedRating[];
}

export interface BoardEntry {
	rank: number;
	user_id: number;
	username: string;
	mmr: number;
	played_at: number;
}

export interface Leaderboard {
	classes: string[];
	class_name: string;
	total: number;
	has_more: boolean;
	entries: BoardEntry[];
}

/** A match in someone's history: the archive row plus their side and line. */
export interface AgentMatch extends MatchSummary {
	task_force: number | null;
	kills: number;
	deaths: number;
	assists: number;
	damage_dealt: number;
	damage_taken: number;
	healing: number;
	obj_points: number;
	time_played_seconds: number;
}

export interface MatchPage {
	kind: string;
	limit: number;
	page: number;
	total: number;
	has_more: boolean;
	matches: AgentMatch[];
}

async function getJson<T>(url: string): Promise<T> {
	const response = await fetch(url, {
		headers: { accept: 'application/json' },
		credentials: 'same-origin'
	});
	if (!response.ok) {
		throw new Error(
			response.status === 404 ? 'not found' : `unavailable (${response.status})`
		);
	}
	return (await response.json()) as T;
}

export function fetchAgents(query: string, page: number, limit = 24) {
	const params = new URLSearchParams({ limit: String(limit), page: String(page) });
	if (query.trim()) params.set('q', query.trim());
	return getJson<DirectoryPage>(`/api/agents?${params.toString()}`);
}

export function fetchAgent(userId: number | string) {
	return getJson<AgentProfile>(`/api/agents/${userId}`);
}

export function fetchAgentMatches(
	userId: number | string,
	kind: 'all' | 'pvp' | 'pve',
	page: number,
	limit = 10
) {
	const params = new URLSearchParams({ limit: String(limit), page: String(page), kind });
	return getJson<MatchPage>(`/api/agents/${userId}/matches?${params.toString()}`);
}

export function fetchLeaderboard(class_name: string, page: number, limit = 25) {
	const params = new URLSearchParams({
		class: class_name,
		limit: String(limit),
		page: String(page)
	});
	return getJson<Leaderboard>(`/api/leaderboard?${params.toString()}`);
}
