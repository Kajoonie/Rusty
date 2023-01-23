FROM rust:latest
RUN rustup default nightly
RUN cargo install cargo-shuttle