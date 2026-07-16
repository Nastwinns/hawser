# hawser (haw) — minimal multi-stage image.
#
# Build:
#   docker build -t haw .
# Run:
#   docker run --rm haw --version
#   docker run --rm -v "$PWD:/work" -w /work haw sync
#
# Runtime note: haw reads git state through gitoxide, but shells out to the
# system `git` binary for every *mutation* (clone/pull/checkout/branch — see
# crates/haw-git/src/lib.rs, `Command::new("git")`). So `git` MUST be present in
# the final image or any command that touches a repo will fail. We install it,
# plus ca-certificates for HTTPS remotes.

# ---- builder ---------------------------------------------------------------
FROM rust:1.90-slim-bookworm AS builder

# `git` is needed at build time too: some cargo/dep operations and the build
# itself expect it, and it keeps the builder consistent with runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

# Build only the `haw` binary (the `hawser` package). --locked keeps the build
# reproducible against the committed Cargo.lock.
RUN cargo build --release --locked -p hawser

# ---- runtime ---------------------------------------------------------------
# debian:stable-slim keeps the image small while still giving us a real `git`
# and CA certificates at runtime (both required — see the note above).
FROM debian:stable-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends git ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home --uid 10001 haw

COPY --from=builder /src/target/release/haw /usr/local/bin/haw

USER haw
WORKDIR /home/haw

ENTRYPOINT ["haw"]
CMD ["--help"]
