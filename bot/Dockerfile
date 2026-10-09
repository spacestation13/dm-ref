FROM rust:latest AS bot-deps

WORKDIR /usr/src/bot
COPY bot/Cargo.toml bot/Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

FROM bot-deps AS builder

COPY bot/src ./src
COPY bot/build.rs ./
# Scraped pages and the site's search index, placed in bot/data by .github/workflows/deploy.yml
COPY bot/data/content /usr/src/content
COPY bot/data/public/static/search /usr/src/search
RUN touch src/main.rs && SOURCE_DIR=/usr/src/content SEARCH_DIR=/usr/src/search cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/bot/target/release/dm-ref-bot /usr/local/bin/dm-ref-bot

EXPOSE 8080
CMD ["dm-ref-bot"]
