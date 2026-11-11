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
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*

# Unprivileged. The compose file pins this to the uid that owns the game DB on
# the host so the read-only mount is actually readable.
WORKDIR /app
COPY --from=backend /src/target/release/webapi /usr/local/bin/webapi
# The snapshot job shares the image: same archive, different entrypoint.
COPY --from=backend /src/target/release/snapshotter /usr/local/bin/snapshotter
COPY --from=frontend /web/build /app/static

ENV WEB_BIND=0.0.0.0:8080 \
    WEB_STATIC_DIR=/app/static \
    RUST_LOG=info,tower_http=warn

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/api/health || exit 1

ENTRYPOINT ["/usr/local/bin/webapi"]
