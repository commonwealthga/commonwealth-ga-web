# syntax=docker/dockerfile:1

# --- frontend ---------------------------------------------------------------
# Node 22 is required: Tailwind CSS v4's engine needs Node >= 20. Local dev on
# an older Node will fail here identically, so `npm run build` catches it early.
FROM node:22-alpine AS frontend
WORKDIR /web
COPY web/package.json web/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY web/ ./
RUN npm run build

# --- backend ----------------------------------------------------------------
FROM rust:1-slim-bookworm AS backend
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN cargo build --release --locked --bin webapi --bin snapshotter

# --- runtime ----------------------------------------------------------------
# One image, two processes. The website is served by the Node build
# (`adapter-node`), which renders every page on the server so a crawler reads it
# and the browser navigates client-side; the Rust binary behind it answers the
# API, the downloads and the redacted database. Node's base image is Debian
# bookworm, the same as the Rust build, so the binaries run here unmodified.
FROM node:22-bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# The API and the snapshot job (same archive, different entrypoint).
COPY --from=backend /src/target/release/webapi /usr/local/bin/webapi
COPY --from=backend /src/target/release/snapshotter /usr/local/bin/snapshotter
# The site: SvelteKit's Node server. `adapter-node` bundles its dependencies, so
# there is nothing to install at runtime.
COPY --from=frontend /web/build /app/build

ENV WEB_BIND=0.0.0.0:8080 \
    PORT=3000 \
    RUST_LOG=info,tower_http=warn

EXPOSE 8080 3000

# The API's own health endpoint. The `web` service overrides this (a Node process
# has no such endpoint) — see the healthcheck there.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/webapi"]
