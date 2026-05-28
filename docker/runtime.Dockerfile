FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock /app/
COPY src /app/src
RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --shell /usr/sbin/nologin octobot
COPY --from=builder /app/target/release/OctoBot /usr/local/bin/octobot
USER octobot
EXPOSE 7879
HEALTHCHECK --interval=30s --timeout=5s --retries=3 CMD curl -fsS http://127.0.0.1:7879/runtime/health
CMD ["octobot"]
