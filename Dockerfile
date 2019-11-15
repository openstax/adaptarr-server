FROM rust:1.37 as build
# --- Install dependencies -----------------------------------------------------
#
# We do this before cache and compilation, since our native dependencies will
# change least often.
RUN apt-get update && apt-get install -y \
    libmagic-dev
# --- Cache dependencies -------------------------------------------------------
#
# Docker cache is invalidated each time a single file changes. To avoid loosing
# cargo's dependency cache each time our source changes (which should happen
# more often than change in dependencies) we create an intermediate image
# containing just an empty project with our dependencies.
# Create an empty shell project
RUN USER=root cargo new --vcs none --bin /usr/src/adaptarr/crates/cli --name adaptarr-cli \
 && USER=root cargo new --vcs none --bin /usr/src/adaptarr/crates/conversations --name adaptarr-conversations \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/error --name adaptarr-error \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/i18n --name adaptarr-i18n \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/macros --name adaptarr-macros \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/mail --name adaptarr-mail \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/models --name adaptarr-models \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/pages --name adaptarr-pages \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/rest-api --name adaptarr-rest-api \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/util --name adaptarr-util \
 && USER=root cargo new --vcs none --lib /usr/src/adaptarr/crates/web --name adaptarr-web
WORKDIR /usr/src/adaptarr
# Copy over manifests
COPY ./Cargo.toml ./Cargo.toml
COPY ./crates/cli/Cargo.toml ./crates/cli/Cargo.toml
COPY ./crates/error/Cargo.toml ./crates/error/Cargo.toml
COPY ./crates/i18n/Cargo.toml ./crates/i18n/Cargo.toml
COPY ./crates/macros/Cargo.toml ./crates/macros/Cargo.toml
COPY ./crates/mail/Cargo.toml ./crates/mail/Cargo.toml
COPY ./crates/models/Cargo.toml ./crates/models/Cargo.toml
COPY ./crates/pages/Cargo.toml ./crates/pages/Cargo.toml
COPY ./crates/rest-api/Cargo.toml ./crates/rest-api/Cargo.toml
COPY ./crates/util/Cargo.toml ./crates/util/Cargo.toml
COPY ./crates/web/Cargo.toml ./crates/web/Cargo.toml
COPY ./crates/conversations/Cargo.toml ./crates/conversations/Cargo.toml
COPY ./Cargo.lock ./Cargo.lock
# Populate dependency cache
RUN cargo build --all --release \
 && rm -rf src macros/src target/release/**/*adaptarr*
# --- Build project ------------------------------------------------------------
#
# Now that our dependencies are cached we can build application proper.
# Copy sources

COPY ./crates ./crates
COPY ./doc ./doc
COPY ./crates ./crates
COPY ./locales ./locales
COPY ./migrations ./migrations
COPY ./templates ./templates
COPY ./tests ./tests
COPY ./diesel.toml ./diesel.toml
# COPY ./.git ./.git

# Build for release
RUN cargo build --release --bin adaptarr

# --- Create a minimal base image ----------------------------------------------
#
# Our current image contains development files and tools, bloating its size to
# around 1.8GB. Since we don't need all that stuff in production, we can create
# a much smaller image (around 150MB) by starting from a base Debian and only
# pulling in what we need.
#
# We could make even smaller image using Alpine, but building Rust for musl C in
# Docker is not very easy at the moment.
FROM debian:latest
# Install dependencies
RUN apt-get update && apt-get install -y \
    libmagic1 \
    libpq5 \
    libssl1.1 \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*
# --- Create image -------------------------------------------------------------
WORKDIR /var/lib/adaptarr
COPY --from=build /usr/src/adaptarr/target/release/adaptarr /usr/bin/adaptarr
COPY --from=build /usr/src/adaptarr/locales /var/lib/adaptarr/locales
COPY --from=build /usr/src/adaptarr/templates /var/lib/adaptarr/templates
EXPOSE 80
ENV RUST_BACKTRACE=1
ENTRYPOINT ["/usr/bin/adaptarr"]

COPY ./config.toml ./config.toml