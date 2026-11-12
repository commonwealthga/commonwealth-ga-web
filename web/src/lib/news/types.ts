/** Shapes the news API returns, in one place so the pages and components agree. */

/** Set on the automatic announcement of a published guide or build. */
export interface GuidePointer {
	id: number;
	category: 'guide' | 'build';
	title: string;
	path: string;
	class_label: string | null;
	image_url: string | null;
}

export interface NewsPost {
	id: number;
	kind: 'news' | 'update' | 'article' | 'video';
	kind_label: string;
	title: string;
	slug: string;
	path: string;
	body_html: string;
	body_markdown?: string;
	link: string | null;
	videos: string[];
	author: string;
	created_at: number;
	thumbs_up: number;
	reacted: boolean;
	status: 'pending' | 'approved' | 'rejected';
	/** Only its author, only while pending — decided by the API. */
	can_edit: boolean;
	moderation_note?: string | null;
	/** Present on the automatic announcement of a published guide or build. */
	guide?: GuidePointer;
}

export interface Allowance {
	limit: number;
	used: number;
	remaining: number;
	resets_at: number | null;
	resets_at_clock: string | null;
}

export interface PostingRules {
	mode: string;
	signed_in: boolean;
	moderator: boolean;
	may_post: boolean;
	reason: string | null;
	allowance: Allowance | null;
}

/** What `/api/news` answers: the whole feed view, not just a page of it. */
export interface FeedResponse {
	/** The kind the list was narrowed to, when it was. */
	filter?: { kind: 'news' | 'update' | 'article' | 'video' | null };
	posts: NewsPost[];
	pending: NewsPost[];
	total: number;
	offset: number;
	moderator: boolean;
	posting: PostingRules;
}

export interface RulesResponse extends PostingRules {
	kinds: {
		value: string;
		label: string;
		phrase: string;
		requires_link: boolean;
		requires_body: boolean;
		has_editor: boolean;
	}[];
	limits: {
		title_max: number;
		body_max: number;
		per_day: number;
		max_videos: number;
	};
}

export type ModeratorAction = 'approve' | 'reject' | 'unapprove' | 'note';
