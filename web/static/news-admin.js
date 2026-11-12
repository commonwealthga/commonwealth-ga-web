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
	var thumb = document.getElementById('thumb');
	if (thumb) {
		var thumbCount = document.getElementById('thumb-count');
		thumb.addEventListener('click', function () {
			var wanted = thumb.dataset.active !== 'true';
			post('/api/news/' + thumb.dataset.post + '/thumb', { active: wanted })
				.then(function (data) {
					thumbCount.textContent = String(data.thumbs_up);
					thumb.dataset.active = String(data.reacted);
					thumb.setAttribute('aria-pressed', String(data.reacted));
				})
				.catch(function (error) {
					// Most often "not signed in", which is worth saying rather than
					// leaving a button that appears broken.
					var note = document.getElementById('thumb-note');
					if (note) note.textContent = error.message;
				});
		});
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
