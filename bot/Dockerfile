FROM rust:latest AS scraper

WORKDIR /usr/src
RUN apt-get update && apt-get install -y wget unzip && rm -rf /var/lib/apt/lists/*

RUN wget https://byond-builds.dm-lang.org/516/516.latest_byond.zip -O byond.zip && unzip byond.zip

COPY scraper /usr/src/scraper
WORKDIR /usr/src/scraper
RUN cargo run --release -- --ref /usr/src/byond/help/ref/info.html --output /usr/src/content

FROM rust:latest AS builder

COPY bot /usr/src/bot
COPY --from=scraper /usr/src/content /usr/src/content
WORKDIR /usr/src/bot

RUN SOURCE_DIR=/usr/src/content cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/bot/target/release/dm-ref-bot /usr/local/bin/dm-ref-bot

EXPOSE 8080
CMD ["dm-ref-bot"]
