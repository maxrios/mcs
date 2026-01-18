FROM rust:latest as builder

WORKDIR /usr/src/mcs

COPY . .

RUN cargo build --release -p server

FROM debian:trixie-slim

RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/mcs/target/release/server /app/server

RUN mkdir tls

EXPOSE 64400

CMD ["./server"]
