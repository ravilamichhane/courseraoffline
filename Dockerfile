# syntax=docker/dockerfile:1

FROM node:22-bookworm-slim AS frontend-builder
WORKDIR /app/frontend
RUN corepack enable
COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm run build

FROM rust:1-bookworm AS backend-builder
WORKDIR /app
COPY Cargo.toml Cargo.lock build.rs ./
COPY migrations ./migrations
COPY src ./src
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist
RUN cargo build --release --bin courseviewer --bin migrate

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend-builder /app/target/release/courseviewer /usr/local/bin/courseviewer
COPY --from=backend-builder /app/target/release/migrate /usr/local/bin/migrate
RUN mkdir -p /data/downloads

ENV DATA_DIR=/data
ENV MODE=production
ENV HOST=0.0.0.0
ENV PORT=8080

VOLUME ["/data"]
EXPOSE 8080

CMD ["courseviewer"]
