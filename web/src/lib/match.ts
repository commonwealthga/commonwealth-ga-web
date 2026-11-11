/**
 * Shapes returned by `/api/matches/{instance_id}`.
 *
 * One payload, two layouts: `kind` decides which. PvP carries rating context
 * (MMR per player, team balance) that PvE matches do not have, and PvE carries
 * the mission detail that a match with no rating does not need to hide behind.
 */

export type MatchKind = 'pvp' | 'pve';

export interface TeamSummary {
	task_force: number;
	label: string;
	players: number;
	kills: number;
	deaths: number;
	damage_dealt: number;
	healing: number;
	obj_points: number;
	bot_kills: number;
	won: boolean;
}

export interface Participant {
	user_id: number;
	player: string;
	task_force: number | null;
	classes: string[];
	mmr: number | null;
	/** What the match did to that rating; null when the engine recorded none. */
	mmr_delta: number | null;
	mmr_class: string | null;
	won: boolean;
	is_viewer: boolean;
	rep_points: number;
	kills: number;
	assists: number;
	deaths: number;
	damage_dealt: number;
	damage_taken: number;
	healing: number;
	buff_value: number;
	defense: number;
	obj_points: number;
	bot_kills: number;
	objective_captures: number;
	beacon_spawns_provided: number;
	beacon_spawns_used: number;
	beacons_destroyed: number;
	capture_seconds: number;
	contest_seconds: number;
	time_played_seconds: number;
}

export interface Balance {
	wt_avg: number;
	lt_avg: number;
	delta: number;
	wt_prob: number;
}

export interface Viewer {
	user_id: number;
	task_force: number | null;
	won: boolean;
	draw: boolean;
}

export interface MatchDetail {
	instance_id: number;
	kind: MatchKind;
	map_name: string;
	map_label: string | null;
	queue_code: string | null;
	queue_label: string | null;
	queue_location: string | null;
	queue_description: string | null;
	game_mode: string;
	outcome: string | null;
	winning_task_force: number | null;
	started_at: number;
	sealed_at: number | null;
	duration_seconds: number | null;
	teams: TeamSummary[];
	participants: Participant[];
	balance: Balance | null;
	mmr_engine: string | null;
	viewer: Viewer | null;
}

export async function fetchMatchDetail(instanceId: string | number): Promise<MatchDetail> {
	const response = await fetch(`/api/matches/${instanceId}`, {
		headers: { accept: 'application/json' },
		credentials: 'same-origin'
	});
	if (!response.ok) {
		throw new Error(
			response.status === 404 ? 'This match is not in the archive.' : `unavailable (${response.status})`
		);
	}
	return (await response.json()) as MatchDetail;
}
