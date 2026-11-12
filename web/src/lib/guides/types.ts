/**
 * The shapes `/api/guides` returns.
 *
 * Kept next to the API calls rather than inside components, so a change in
 * `crates/webapi/src/routes/guides.rs` has exactly one place to land here.
 */

export type Category = 'guide' | 'build';
export type Status = 'pending' | 'approved' | 'rejected';
export type Sort = 'helpful' | 'newest';

export interface Entry {
	id: number;
	category: Category;
	category_label: string;
	title: string;
	slug: string;
	/** `/guides/12-assault-basics` — the canonical address of the detail page. */
	path: string;
	summary: string;
	body_html: string;
	/** Only for the author and for moderators. */
	body_markdown?: string;
	tags: string[];
	tag_labels: string[];
	build_class: string | null;
	class_label: string | null;
	modes: string[];
	mode_labels: string[];
	discord_url: string | null;
	/** A build's sanitised screenshot: `/images/<hash>.webp`. */
	image_url: string | null;
	/** The stored image's id, resent on an edit that keeps the same screenshot. */
	image_id: number | null;
	author: string;
	created_at: number;
	thumbs_up: number;
	reacted: boolean;
	status: Status;
	/** Only its author, only while pending — decided by the API. */
	can_edit: boolean;
	moderation_note?: string;
}

export interface Allowance {
	limit: number;
	used: number;
	remaining: number;
	resets_at?: number;
	resets_at_clock?: string;
}

export interface Posting {
	mode: string;
	signed_in: boolean;
	may_post: boolean;
	reason?: string;
	allowance?: Allowance;
}

export interface FeedResponse {
	entries: Entry[];
	pending: Entry[];
	total: number;
	offset: number;
	moderator: boolean;
	posting: Posting;
	filter: {
		category: Category;
		tag: string | null;
		class: string | null;
		mode: string | null;
		q: string;
		sort: Sort;
	};
}

export interface DetailResponse {
	entry: Entry;
	moderator: boolean;
	/** A rejected entry, kept for the record and visible only to its author. */
	archived: boolean;
}

export interface Option {
	value: string;
	label: string;
}

export interface CategoryOption extends Option {
	plural: string;
	path: string;
	requires_image: boolean;
	uses_tags: boolean;
}

export interface OptionsResponse {
	categories: CategoryOption[];
	tags: Option[];
	classes: Option[];
	modes: Option[];
	limits: {
		title_min: number;
		title_max: number;
		summary_max: number;
		body_max: number;
		description_max: number;
		per_day: number;
		max_upload_bytes: number;
		max_upload_width: number;
	};
	posting: Posting;
	formatting_help: string;
}

export interface UploadedImage {
	image_id: number;
	url: string;
	width: number;
	height: number;
	bytes: number;
}

export interface Draft {
	category: Category;
	title: string;
	summary: string;
	body: string;
	tags: string[];
	class: string;
	modes: string[];
	discord: string;
	image_id: number | null;
}
