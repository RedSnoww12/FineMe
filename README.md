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
4. Ouvrir l'URL Vercel : l'écran de première configuration crée l'unique compte. Il demande le
   **code de configuration** : la valeur de `SETUP_TOKEN` générée par Render (onglet *Environment* du service). Les données peuvent ensuite
   être chargées depuis *Paramètres > Importer* (fichier d'export FineMe).

### Mot de passe

Le mot de passe se change dans *Paramètres > Mot de passe* ; les autres appareils sont alors déconnectés.
Mot de passe perdu : lien *Mot de passe oublié* de l'écran de connexion, avec le **code de secours** =
la valeur de `RECOVERY_TOKEN` générée par Render (onglet *Environment*). Sans cette variable, la récupération
est désactivée. Toutes les sessions ouvertes sont fermées après une réinitialisation.

L'offre gratuite de Render met l'API en veille après inactivité : la première requête peut prendre près d'une minute.

### Cours et instantané quotidiens

Le workflow [`daily.yml`](.github/workflows/daily.yml) appelle chaque matin `POST /api/jobs/daily`, qui
rafraîchit les cours puis enregistre l'instantané de patrimoine du jour. À configurer une fois dans
*Settings > Secrets and variables > Actions* du dépôt : la **variable** `FINEME_API_URL` (URL Render de l'API) et
le **secret** `CRON_SECRET` (valeur de `CRON_SECRET` générée par Render, onglet *Environment*). Il se lance aussi
à la main depuis l'onglet *Actions* (*Run workflow*). Tout autre planificateur capable d'envoyer cette requête
convient (ex. cron-job.org).

Cours des titres : Yahoo Finance, puis **EODHD** en repli si `EODHD_API_KEY` est renseignée sur Render (clé
gratuite sur eodhd.com, nombre d'appels par jour limité ; le repli n'est appelé que pour les titres que Yahoo n'a
pas cotés). Un cours automatique qui n'a pas été mis à jour depuis plus de 4 jours est signalé « ancien » dans
la liste des positions.

## Connexions bancaires

Aucun agrégateur n'est branché. L'application fonctionne en saisie manuelle et import CSV ; ajouter un agrégateur
revient à implémenter le port `BankProvider` (`api/crates/fineme-application/src/ports.rs`) dans `fineme-infra`.
