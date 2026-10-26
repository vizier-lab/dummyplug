FROM rust:1 AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
COPY --from=build /src/target/release/dummyplug /usr/local/bin/dummyplug
ENV DUMMYPLUG_BIND=0.0.0.0:9998
EXPOSE 9998
CMD ["dummyplug"]
