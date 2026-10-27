# ServerStatus-Rust (fork) panel image for haruka.
# The binary is built on the builder machine (glibc) and shipped in, so no
# Rust toolchain is needed on the target host (haruka disk is tight).
# Ubuntu 24.04, not alpine/bookworm: the local build links against
# glibc 2.38 (local toolchain), which bookworm's 2.36 cannot satisfy and
# musl cannot run at all (no musl-gcc here for a static build).
# Ubuntu 24.04 ships glibc 2.39.
FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates tzdata \
    && rm -rf /var/lib/apt/lists/*
COPY stat_server /usr/local/bin/stat_server
RUN chmod +x /usr/local/bin/stat_server
EXPOSE 8080 9394
ENTRYPOINT ["/usr/local/bin/stat_server"]
