# Image de l'API FineMe (workspace Cargo, binaire `fineme-server`). Contexte de build : le dossier `api/`.

# --- build
FROM rust:1-slim-bookworm AS build
WORKDIR /app
# cmake/clang : compilation d'aws-lc (TLS rustls).
RUN apt-get update && apt-get install -y --no-install-recommends cmake clang make pkg-config \
    && rm -rf /var/lib/apt/lists/*

# Couche de cache des dépendances : manifestes seuls + sources vides.
COPY Cargo.toml Cargo.lock ./
COPY crates/fineme-domain/Cargo.toml crates/fineme-domain/
COPY crates/fineme-application/Cargo.toml crates/fineme-application/
COPY crates/fineme-infra/Cargo.toml crates/fineme-infra/
COPY crates/fineme-http/Cargo.toml crates/fineme-http/
COPY crates/fineme-mcp/Cargo.toml crates/fineme-mcp/
COPY crates/fineme-server/Cargo.toml crates/fineme-server/
RUN for crate in domain application infra http mcp server; do \
        mkdir -p crates/fineme-$crate/src && : > crates/fineme-$crate/src/lib.rs; \
    done \
    && echo "fn main() {}" > crates/fineme-server/src/main.rs \
    && cargo build --release --locked -p fineme-server

# Sources réelles (migrations SQL comprises, embarquées dans le binaire).
COPY crates ./crates
RUN find crates -name '*.rs' -exec touch {} + && cargo build --release --locked -p fineme-server

# --- runtime
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* && useradd --system --uid 10001 fineme
COPY --from=build /app/target/release/fineme-server /usr/local/bin/fineme-server
USER fineme
# Render fournit $PORT ; le serveur écoute sur 0.0.0.0:$PORT (8080 par défaut).
ENV PORT=8080
EXPOSE 8080
CMD ["fineme-server"]
