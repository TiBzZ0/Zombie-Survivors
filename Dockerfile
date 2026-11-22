# Image du serveur dédié. Construction : docker build -t zombie-survivors-server .

FROM rust:1-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Dépendances seules d'abord : cette couche reste en cache tant que Cargo.lock ne change pas.
RUN cargo chef cook --release -p server --recipe-path recipe.json
COPY . .
RUN cargo build --release -p server --bin server

FROM debian:bookworm-slim
RUN useradd --create-home --uid 1000 game && mkdir /data && chown game:game /data
COPY --from=builder /app/target/release/server /usr/local/bin/server
USER game
ENV DATA_DIR=/data PORT=7777
VOLUME /data
EXPOSE 7777/udp
ENTRYPOINT ["/usr/local/bin/server"]
