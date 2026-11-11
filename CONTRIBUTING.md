# Contributing

Pull requests are welcome — this repository is public so that players can fix
things and add things, not so the site can be run for another server. Please
read the licence note below before you send one.

## The licence, in plain terms

This project is under the [PolyForm Shield License 1.0.0](LICENSE). It is
*source-available*, not open source: anyone may read, copy, modify and run it,
but may not use it to provide a product that competes with The Commonwealth.

Practically, that means:

- your fork, your local experiments and your pull requests are all fine;
- running another Global Agenda community's website on it is a violation;
- the name, wordmark and site art are not covered by the licence at all — see
  [NOTICE](NOTICE).

## Developer Certificate of Origin

Every commit must carry a `Signed-off-by` line. Add it by committing with `-s`:

```sh
git commit -s -m "Fix the mission pager"
```

which produces:

```
Fix the mission pager

Signed-off-by: Your Name <you@example.com>
```

That line is your statement that you wrote the change, or have the right to
submit it, and that it may be distributed under the project's licence — the
[Developer Certificate of Origin 1.1](https://developercertificate.org/).
Please use a real name and a reachable address; the sign-off is per commit, so
amended and rebased commits need it too (`git commit -s --amend`,
`git rebase --signoff`).

## Before you open a pull request

- `cargo test`, `cargo clippy --all-targets -- -D warnings` and `cargo fmt --all`
  for the Rust side.
- `npm run check` and `npm run build` in `web/` for the frontend.
- New routes and helpers get tests, in `crates/webapi/tests/`. Every route must
  degrade to a 404 or 503, never a traceback.
- `/api/status` and `/files/*` are public contracts, polled by the Discord bot
  and the launcher. Changing their shape needs coordination, not just a PR.

Keep the style of the codebase: small top-level helpers for data access, thin
HTTP handlers, no new dependencies without a reason. `CLAUDE.md` has the full
set of project invariants.
