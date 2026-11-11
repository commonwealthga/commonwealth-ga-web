/**
 * In-game credential derivation, reproduced in the browser.
 *
 * Reverse-engineered from the retail client (see
 * `.planning/2026-06-08-login-password-flow.md` in the server repo):
 *
 *     A       = lowercase(account) + password + STATIC_SALT   (UTF-16LE)
 *     H1      = MD5(A)
 *     RC4_KEY = MD5(H1)
 *     plain   = guidString + lowercase(account)                (UTF-16LE, 36 + n chars)
 *     blob    = RC4(RC4_KEY, plain)
 *
 * The server stores SHA256 of the *keystream* (blob XOR plain). We can produce
 * that keystream directly by encrypting zeros, so nothing here depends on the
 * server's challenge and the plaintext password never leaves this function.
 *
 * Plain .mjs on purpose: the exact same file is unit-checked from Node against
 * an independent implementation.
 */

/** The client's static salt, exact case. */
export const STATIC_SALT = 'FA00CDF5-8D87-4a76-BEE1-D9E483220C13';

/** Length of `UuidToStringW()` output, in characters. */
export const GUID_STRING_LEN = 36;

/**
 * UTF-16LE bytes of a string's code units (no BOM, no NUL terminator).
 * @param {string} text
 * @returns {Uint8Array}
 */
export function utf16le(text) {
	const out = new Uint8Array(text.length * 2);
	for (let i = 0; i < text.length; i += 1) {
		const code = text.charCodeAt(i);
		out[i * 2] = code & 0xff;
		out[i * 2 + 1] = (code >>> 8) & 0xff;
	}
	return out;
}

/* --- MD5 ------------------------------------------------------------------
 * Compact implementation (RFC 1321). MD5 is the client's choice, not ours:
 * the game uses CALG_MD5 for both derivation steps.
 * ------------------------------------------------------------------------- */

const S = [
	7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21
];
const K = new Uint32Array(64);
for (let i = 0; i < 64; i += 1) {
	K[i] = Math.floor(Math.abs(Math.sin(i + 1)) * 4294967296) >>> 0;
}

/**
 * @param {Uint8Array} message
 * @returns {Uint8Array} 16-byte digest
 */
export function md5(message) {
	const bitLength = message.length * 8;
	// Pad to 56 bytes mod 64, then append the length as a 64-bit little endian.
	const paddedLength = (((message.length + 8) >>> 6) + 1) << 6;
	const buffer = new Uint8Array(paddedLength);
	buffer.set(message);
	buffer[message.length] = 0x80;
	for (let i = 0; i < 8; i += 1) {
		buffer[paddedLength - 8 + i] = (bitLength / Math.pow(2, 8 * i)) & 0xff;
	}

	let a0 = 0x67452301;
	let b0 = 0xefcdab89;
	let c0 = 0x98badcfe;
	let d0 = 0x10325476;

	const words = new Uint32Array(16);

	for (let offset = 0; offset < paddedLength; offset += 64) {
		for (let i = 0; i < 16; i += 1) {
			words[i] =
				buffer[offset + i * 4] |
				(buffer[offset + i * 4 + 1] << 8) |
				(buffer[offset + i * 4 + 2] << 16) |
				(buffer[offset + i * 4 + 3] << 24);
		}

		let a = a0;
		let b = b0;
		let c = c0;
		let d = d0;

		for (let i = 0; i < 64; i += 1) {
			let f;
			let g;
			if (i < 16) {
				f = (b & c) | (~b & d);
				g = i;
			} else if (i < 32) {
				f = (d & b) | (~d & c);
				g = (5 * i + 1) % 16;
			} else if (i < 48) {
				f = b ^ c ^ d;
				g = (3 * i + 5) % 16;
			} else {
				f = c ^ (b | ~d);
				g = (7 * i) % 16;
			}

			const tmp = d;
			d = c;
			c = b;
			const sum = (a + f + K[i] + words[g]) >>> 0;
			const shift = S[(i >>> 4) * 4 + (i % 4)];
			b = (b + ((sum << shift) | (sum >>> (32 - shift)))) >>> 0;
			a = tmp;
		}

		a0 = (a0 + a) >>> 0;
		b0 = (b0 + b) >>> 0;
		c0 = (c0 + c) >>> 0;
		d0 = (d0 + d) >>> 0;
	}

	const digest = new Uint8Array(16);
	[a0, b0, c0, d0].forEach((word, index) => {
		digest[index * 4] = word & 0xff;
		digest[index * 4 + 1] = (word >>> 8) & 0xff;
		digest[index * 4 + 2] = (word >>> 16) & 0xff;
		digest[index * 4 + 3] = (word >>> 24) & 0xff;
	});
	return digest;
}

/* --- RC4 ----------------------------------------------------------------- */

/**
 * RC4 keystream of `length` bytes for the given key. Producing keystream
 * directly is equivalent to encrypting a run of zeros.
 * @param {Uint8Array} key
 * @param {number} length
 * @returns {Uint8Array}
 */
export function rc4Keystream(key, length) {
	const state = new Uint8Array(256);
	for (let i = 0; i < 256; i += 1) state[i] = i;

	let j = 0;
	for (let i = 0; i < 256; i += 1) {
		j = (j + state[i] + key[i % key.length]) & 0xff;
		const swap = state[i];
		state[i] = state[j];
		state[j] = swap;
	}

	const out = new Uint8Array(length);
	let i = 0;
	j = 0;
	for (let n = 0; n < length; n += 1) {
		i = (i + 1) & 0xff;
		j = (j + state[i]) & 0xff;
		const swap = state[i];
		state[i] = state[j];
		state[j] = swap;
		out[n] = state[(state[i] + state[j]) & 0xff];
	}
	return out;
}

/* --- Derivation ----------------------------------------------------------- */

/**
 * The credential check the game performs, as the browser can reproduce it.
 *
 * Returns the RC4 keystream the server hashes to verify the login. Treat the
 * result as a password: it must only ever be sent over HTTPS.
 *
 * @param {string} username account name as typed
 * @param {string} password plaintext password
 * @returns {Uint8Array}
 */
export function deriveKeystream(username, password) {
	const account = username.toLowerCase();
	const salted = utf16le(account + password + STATIC_SALT);
	const key = md5(md5(salted));
	const length = (GUID_STRING_LEN + account.length) * 2;
	return rc4Keystream(key, length);
}

/**
 * @param {Uint8Array} bytes
 * @returns {string} lowercase hex
 */
export function toHex(bytes) {
	let out = '';
	for (const byte of bytes) out += byte.toString(16).padStart(2, '0');
	return out;
}
