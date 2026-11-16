# Raccourcis pour tout le dépôt. `make` ou `make help` liste les cibles.
# Cargo et npm restent les vrais outils de build ; ce fichier ne fait que les enchaîner.

CARGO := cd api && cargo
NPM   := cd web && npm

.DEFAULT_GOAL := help
.PHONY: help install db db-stop api web dev test test-api test-web e2e lint fmt check build docker clean

help: ## Liste les commandes
	@grep -E '^[a-zA-Z0-9_-]+:.*## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*## "} {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}'

install: ## Installe les dépendances du front et prépare api/.env
	$(NPM) install
	@test -f api/.env || cp api/.env.example api/.env

db: ## Démarre Postgres en local (Docker, port 5433)
	docker compose up -d db

db-stop: ## Arrête Postgres
	docker compose stop db

api: db ## Lance l'API Rust sur :8080
	$(CARGO) run -p fineme-server

web: ## Lance le front sur :5173
	$(NPM) run dev

dev: db ## Lance l'API et le front ensemble (Ctrl+C arrête les deux)
	@trap 'kill 0' INT TERM; \
	(cd api && cargo run -p fineme-server) & \
	(cd web && npm run dev) & \
	wait

test: test-api test-web ## Tests unitaires et d'intégration (API + front)

test-api: ## Tests Rust (Docker requis pour l'intégration)
	$(CARGO) test --workspace

test-web: ## Tests du front (Vitest)
	$(NPM) test

e2e: ## Tests Playwright (pile démarrée, base vide)
	$(NPM) run e2e

lint: ## Clippy et oxlint
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(NPM) run lint

fmt: ## Formate le code Rust
	$(CARGO) fmt --all

check: ## Tout ce que la CI vérifie, sauf les E2E
	$(CARGO) fmt --all --check
	$(MAKE) lint
	$(NPM) run build
	$(MAKE) test

build: ## Build de production (binaire Rust + front)
	$(CARGO) build --release -p fineme-server
	$(NPM) run build

docker: ## Construit l'image Docker de l'API (celle de Render)
	docker build -t fineme-api api

clean: ## Supprime les artefacts de build
	$(CARGO) clean
	rm -rf web/dist
