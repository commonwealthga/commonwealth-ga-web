// Interactions for the server-rendered news pages.
//
// Why a plain file rather than an inline <script>: the feed and post pages are
// rendered by Rust, and generating JavaScript inside a `format!` string is how
// the approve button came to be wired to nothing — the escaping silently
// corrupted the script and the browser dropped it with a syntax error. Here it
// is a normal file, which `node --check` (and CI) can actually read.
//
// Loaded with `defer`, so the DOM exists by the time this runs. No framework:
// the pages exist to be crawlable HTML first, interactive second.

(function () {
	'use strict';

	function post(path, body) {
		return fetch(path, {
			method: 'POST',
			credentials: 'same-origin',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify(body)
		}).then(function (response) {
			if (response.ok) return response.json();
			return response
				.json()
				.catch(function () {
					return {};
				})
				.then(function (data) {
					throw new Error(data.error || 'request failed (' + response.status + ')');
				});
		});
	}

	// ---- thumbs up ---------------------------------------------------------
	// One per entry: the feed shows every post in full, each with its own button.
	document.querySelectorAll('[data-thumb-post]').forEach(function (button) {
		var id = button.dataset.thumbPost;
		var count = document.querySelector('[data-thumb-count="' + id + '"]');
		var note = document.querySelector('[data-thumb-note="' + id + '"]');

		button.addEventListener('click', function () {
			var wanted = button.dataset.thumbActive !== 'true';
			post('/api/news/' + id + '/thumb', { active: wanted })
				.then(function (data) {
					if (count) count.textContent = String(data.thumbs_up);
					button.dataset.thumbActive = String(data.reacted);
					button.setAttribute('aria-pressed', String(data.reacted));
					if (note) note.textContent = '';
				})
				.catch(function (error) {
					// Most often "not signed in", which is worth saying rather than
					// leaving a button that appears broken.
					if (note) note.textContent = error.message;
				});
		});
	});

	// ---- endless scroll ----------------------------------------------------
	// The first batch is server-rendered; older entries arrive as the same markup
	// from /news/cards, so there is nothing to keep in step. The button underneath
	// does the same thing, and works without this script.
	var more = document.querySelector('[data-news-more]');
	if (more) {
		var list = document.querySelector('main ul');
		var button = more.querySelector('[data-news-load]');
		var loading = false;

		var loadOlder = function () {
			if (loading || !more) return;
			loading = true;
			if (button) button.disabled = true;
			var offset = Number(more.dataset.offset || '0');

			fetch('/news/cards?offset=' + offset, { credentials: 'same-origin' })
				.then(function (response) {
					if (!response.ok) throw new Error('could not load more (' + response.status + ')');
					var more_ = response.headers.get('X-News-More') === 'true';
					return response.text().then(function (html) { return { html: html, more: more_ }; });
				})
				.then(function (result) {
					var holder = document.createElement('div');
					holder.innerHTML = result.html;
					var added = holder.children.length;
					if (list) {
						while (holder.firstElementChild) list.appendChild(holder.firstElementChild);
					}
					offset += added;
					more.dataset.offset = String(offset);
					var count = more.querySelector('.label');
					if (count && added) {
						// "Showing N of M" — M stays as the server said.
						var match = /of (\d+)/.exec(count.textContent || '');
						if (match) count.textContent = 'Showing ' + offset + ' of ' + match[1];
					}
					if (!result.more || added === 0) {
						more.remove();
						more = null;
					}
				})
				.catch(function (error) {
					var count = more && more.querySelector('.label');
					if (count) count.textContent = error.message;
				})
				.then(function () {
					loading = false;
					if (button) button.disabled = false;
				});
		};

		if (button) button.addEventListener('click', loadOlder);

		// Scroll to the bottom and the next batch starts loading; the button stays
		// as the fallback for a slow connection or a keyboard.
		if ('IntersectionObserver' in window) {
			new IntersectionObserver(function (entries) {
				entries.forEach(function (entry) {
					if (entry.isIntersecting) loadOlder();
				});
			}, { rootMargin: '400px' }).observe(more);
		}
	}

	// ---- moderation --------------------------------------------------------
	// Every action asks before it acts, and the two that take an entry out of the
	// feed also ask for a reason — that reason is the only thing the author sees.
	var CONFIRM = {
		approve: 'publish this entry? It becomes visible to everyone.',
		unapprove: 'unpublish this entry? It returns to the queue and the author can fix it.',
		reject: 'delete this entry? It disappears from the feed everywhere and is kept only for review.'
	};

	var REASON = {
		unapprove: 'What should the author fix? (shown to them)',
		reject: 'Why is this being deleted? (kept with the entry)'
	};

	document.querySelectorAll('[data-action]').forEach(function (button) {
		var card = button.closest('[data-post-title]');
		if (!card) return;
		var id = (card.id || '').replace('post-', '');
		var title = card.dataset.postTitle || 'this entry';
		var message = card.querySelector('[data-message]');

		function say(text) {
			if (!message) return;
			message.textContent = text;
		}

		button.addEventListener('click', function () {
			var action = button.dataset.action;

			// Confirm first: it is the answer to "did you mean that", and it is
			// mandatory for every one of these buttons.
			if (!window.confirm('About to ' + CONFIRM[action] + '\n\n"' + title + '"')) return;

			var note = '';
			if (REASON[action]) {
				var answer = window.prompt(REASON[action], '');
				// Cancel means "never mind", not "no reason": an empty reason would
				// leave the author with nothing to act on.
				if (answer === null) return;
				note = answer.trim();
				if (!note) {
					say('A reason is required for that.');
					return;
				}
			}

			card.querySelectorAll('[data-action]').forEach(function (b) {
				b.disabled = true;
			});
			post('/api/news/' + id + '/' + action, { note: note })
				.then(function () {
					// Server-rendered: reload so the feed shows the result.
					window.location.reload();
				})
				.catch(function (error) {
					card.querySelectorAll('[data-action]').forEach(function (b) {
						b.disabled = false;
					});
					say(error.message);
				});
		});
	});

})();
