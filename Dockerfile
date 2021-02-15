FROM rust:latest as builder
WORKDIR /plate-handler
COPY . .
RUN cargo build --release
RUN cargo install --path .

FROM ubuntu:focal
RUN apt-get update && apt-get install libssl1.1 libsqlite3-dev -qqy
COPY --from=builder /plate-handler/target/release/plate-handler .
EXPOSE 8402
CMD ["./plate-handler"]
