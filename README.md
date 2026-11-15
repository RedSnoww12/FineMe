# FineMe

Suivi de patrimoine personnel pour un foyer : comptes, livrets, PEA/CTO/assurance-vie, crypto, immobilier et prêts,
objectifs, projection financière et simulateur de frais. Un seul login, plusieurs membres avec leurs parts de détention.

- `api/` — API Rust (Axum, sqlx, architecture hexagonale) → Render
- `web/` — React + Vite + TypeScript → Vercel
- Postgres → Supabase

Architecture, conventions et stratégie de test : [CLAUDE.md](CLAUDE.md). Contrat d'API : [docs/API.md](docs/API.md).

## Développement local

```bash
docker compose up -d db
cd api && cp .env.example .env && cargo run -p fineme-server   # http://localhost:8080
cd web && npm install && npm run dev                           # http://localhost:5173
```

Tests : `cargo test --workspace` (Docker requis), `npm test`, `npm run e2e` (pile démarrée, base vide).

## Déploiement

1. **Supabase** — créer un projet, puis copier la chaîne de connexion **Session pooler** (port 5432) depuis
   *Connect*, en ajoutant `?sslmode=require`. Ne pas utiliser le transaction pooler (port 6543).
2. **Render** — *New > Blueprint*, choisir ce dépôt (`render.yaml`). Renseigner `DATABASE_URL` (étape 1) et
   `CORS_ORIGINS` (URL Vercel de l'étape 3, à compléter ensuite). `JWT_SECRET` est généré. Les migrations
   s'exécutent au démarrage.
3. **Vercel** — importer ce dépôt avec `web` comme *Root Directory* (preset Vite) et la variable
   `VITE_API_URL` = URL Render de l'API (sans `/api`). Reporter ensuite l'URL Vercel dans `CORS_ORIGINS` sur Render.
4. Ouvrir l'URL Vercel : l'écran de première configuration crée l'unique compte. Les données peuvent ensuite
   être chargées depuis *Paramètres > Importer* (fichier d'export FineMe).

L'offre gratuite de Render met l'API en veille après inactivité : la première requête peut prendre près d'une minute.

## Connexions bancaires

Aucun agrégateur n'est branché. L'application fonctionne en saisie manuelle et import CSV ; ajouter un agrégateur
revient à implémenter le port `BankProvider` (`api/crates/fineme-application/src/ports.rs`) dans `fineme-infra`.
