# Euterpe server (Phase 2) — no frontend bundle yet
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
ENV EUTERPE_BIND=0.0.0.0:8080
ENV EUTERPE_DATABASE_URL=sqlite:/data/library.db?mode=rwc
EXPOSE 8080
VOLUME ["/data", "/music"]
USER 1000:1000
ENTRYPOINT ["euterpe-server"]
