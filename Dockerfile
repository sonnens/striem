FROM rust:bookworm AS build

RUN apt-get update && apt-get -y install protobuf-compiler nodejs libssl-dev

WORKDIR /app

COPY Cargo.toml Cargo.toml
COPY lib lib
COPY src src

RUN cargo build --release

FROM node:current AS ui-build

COPY ui /ui
WORKDIR /ui
ENV NEXT_PUBLIC_BASE_PATH=/ui
ENV NEXT_PUBLIC_API_URL=/api/1
RUN npm install
RUN npm run build

FROM debian:bookworm-slim
COPY --from=build /usr/lib/*/libssl.so.3 /lib/x86_64-linux-gnu/libssl.so.3
COPY --from=build /usr/lib/*/libssl.so.3 /lib/aarch64-linux-gnu/libssl.so.3
COPY --from=build /usr/lib/*/libcrypto.so.3 /lib/x86_64-linux-gnu/libcrypto.so.3
COPY --from=build /usr/lib/*/libcrypto.so.3 /lib/aarch64-linux-gnu/libcrypto.so.3
COPY --from=build /app/target/release/striem /striem
COPY --from=ui-build /ui/out /usr/share/ui

ENV RUST_LOG=info
ENV STRIEM_API_UI_PATH=/usr/share/ui

EXPOSE 8080
EXPOSE 3000

ENTRYPOINT ["/striem"]
