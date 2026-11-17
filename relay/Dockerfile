# xmas-relay: the village's own relay for online play.
# Build from the relay/ folder:  docker build -t xmas-relay .
FROM rust:1-alpine AS build
RUN apk add --no-cache musl-dev
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release --locked 2>/dev/null || cargo build --release

FROM alpine:3.20
RUN adduser -D -H -u 10001 relay && mkdir -p /data && chown relay /data
COPY --from=build /src/target/release/xmas-relay /usr/local/bin/xmas-relay
USER relay
ENV RELAY_PORT=8080 ADMIN_PORT=8081 RELAY_DATA=/data TRUST_CF=1
EXPOSE 8080 8081
VOLUME /data
HEALTHCHECK --interval=30s --timeout=3s CMD wget -q -O- http://127.0.0.1:8080/health >/dev/null || exit 1
CMD ["xmas-relay"]
