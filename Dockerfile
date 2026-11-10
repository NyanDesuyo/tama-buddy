FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY web ./web
RUN cargo build --release --locked

FROM debian:bookworm-slim
RUN mkdir -p /data && chown 10001:10001 /data
COPY --from=builder /app/target/release/tama-buddy /usr/local/bin/tama-buddy
USER 10001:10001
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/tama-buddy", "--status", "--url", "http://127.0.0.1:3000"]
ENTRYPOINT ["/usr/local/bin/tama-buddy"]
CMD ["--server", "--bind", "0.0.0.0:3000", "--db", "/data/pet.sqlite3"]
