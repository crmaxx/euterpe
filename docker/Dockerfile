# Euterpe — Rust API + React SPA
FROM node:22-bookworm AS frontend
WORKDIR /app/frontend
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
RUN npm run generate:api && npm run build

FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.toml
COPY crates crates
COPY migrations migrations
COPY openapi openapi
RUN cargo build --release -p euterpe-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/euterpe-server /usr/local/bin/euterpe-server
COPY --from=frontend /app/frontend/dist /usr/share/euterpe/static
ENV EUTERPE_BIND=0.0.0.0:8080
ENV EUTERPE_DATABASE_URL=sqlite:/data/library.db?mode=rwc
ENV EUTERPE_STATIC_DIR=/usr/share/euterpe/static
EXPOSE 8080
VOLUME ["/data", "/music"]
USER 1000:1000
ENTRYPOINT ["euterpe-server"]
