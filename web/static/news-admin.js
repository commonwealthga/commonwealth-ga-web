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

	// ---- moderation queue --------------------------------------------------
	document.querySelectorAll('[data-pending]').forEach(function (card) {
		var note = card.querySelector('[data-note]');
		var message = card.querySelector('[data-message]');
		var buttons = card.querySelectorAll('[data-action]');

		function say(text) {
			if (!message) return;
			message.textContent = text;
			message.classList.remove('hidden');
		}

		function setBusy(busy) {
			buttons.forEach(function (button) {
				button.disabled = busy;
			});
		}

		buttons.forEach(function (button) {
			button.addEventListener('click', function () {
				var action = button.dataset.action;
				var value = note ? note.value.trim() : '';

				// The server refuses this too; catching it here saves a round trip.
				if (action === 'reject' && !value) {
					say('A note is required to reject: say what was wrong with it.');
					if (note) note.focus();
					return;
				}

				setBusy(true);
				post('/api/news/' + card.dataset.pending + '/' + action, { note: value })
					.then(function () {
						// Server-rendered: reload so the feed shows the result.
						window.location.reload();
					})
					.catch(function (error) {
						setBusy(false);
						say(error.message);
					});
			});
		});
	});
})();
