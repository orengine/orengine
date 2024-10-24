ARG RUST_VERSION=1.81.0

FROM rust:${RUST_VERSION}-alpine AS build

RUN apk add --no-cache clang lld musl-dev git

COPY . .

CMD ["cargo",  "test", "--", "--nocapture"]
