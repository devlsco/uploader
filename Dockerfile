# Builds everything from source: docker build -t uploader .
FROM node:24-slim AS web
WORKDIR /web
RUN corepack enable
COPY web/package.json web/pnpm-lock.yaml web/pnpm-workspace.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
ENV NEXT_TELEMETRY_DISABLED=1
RUN pnpm build

FROM rust:1-slim-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY --from=web /web/out ./web/out
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates ffmpeg && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 1000 --home /data uploader && mkdir /data && chown uploader /data
COPY --from=server /src/target/release/uploader /usr/local/bin/uploader
USER uploader
ENV UPLOADER_DATA=/data UPLOADER_BIND=0.0.0.0:3000
VOLUME /data
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s CMD ["uploader", "health"]
CMD ["uploader", "serve"]
