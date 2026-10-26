# ServerStatus-Rust (fork) panel image for haruka.
# The binary is built on the builder machine (glibc) and shipped in, so no
# Rust toolchain is needed on the target host (haruka disk is tight).
# Debian slim, not alpine: the local build has no musl-gcc, so the binary
# is dynamically linked against glibc and would not run on musl.
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
COPY stat_server /usr/local/bin/stat_server
RUN chmod +x /usr/local/bin/stat_server
EXPOSE 8080 9394
ENTRYPOINT ["/usr/local/bin/stat_server"]
