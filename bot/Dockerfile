FROM rust:latest AS scraper-deps

WORKDIR /usr/src/scraper
COPY scraper/Cargo.toml scraper/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

FROM scraper-deps AS scraper

RUN apt-get update && apt-get install -y wget unzip && rm -rf /var/lib/apt/lists/*
RUN wget https://byond-builds.dm-lang.org/516/516.latest_byond.zip -O /tmp/byond.zip && unzip /tmp/byond.zip -d /tmp && rm /tmp/byond.zip

COPY scraper/src ./src
RUN touch src/main.rs && cargo build --release
RUN ./target/release/dm-ref-scraper --ref /tmp/byond/help/ref/info.html --output /usr/src/content

FROM node:22 AS frontend-deps

RUN npm install -g pnpm@11

WORKDIR /usr/src/dm-playground
COPY dm-playground/package.json dm-playground/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

FROM frontend-deps AS frontend

COPY dm-playground/ ./
ARG VITE_DISCORD_CLIENT_ID
ARG VITE_BOT_BACKEND_URL=""
RUN pnpm build

FROM rust:latest AS bot-deps

WORKDIR /usr/src/bot
COPY bot/Cargo.toml bot/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

FROM bot-deps AS builder

COPY bot/src ./src
COPY bot/build.rs ./
COPY --from=scraper /usr/src/content /usr/src/content
RUN touch src/main.rs && SOURCE_DIR=/usr/src/content cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/bot/target/release/dm-ref-bot /usr/local/bin/dm-ref-bot
COPY --from=frontend /usr/src/dm-playground/dist /usr/local/share/dm-playground

EXPOSE 8080
CMD ["dm-ref-bot"]
