# FineMe

Application personnelle de suivi de patrimoine pour un foyer : un seul login, plusieurs membres
(chaque compte a des détenteurs avec un pourcentage de détention). Comptes bancaires, livrets,
PEA/CTO/assurance-vie, crypto, immobilier et prêts, objectifs, projection financière, simulateur de frais.
Interface en français, devise de référence EUR.

## Dépôt

| Dossier | Rôle | Hébergement |
|---|---|---|
| `api/` | API REST Rust (workspace Cargo, architecture hexagonale) | Render (Docker) |
| `web/` | Front React + Vite + TypeScript | Vercel |
| `docs/API.md` | Contrat d'API — **source de vérité** entre front et back | – |
| `seed/private/` | Données personnelles, ignoré par Git | – |

Base de données : Postgres (Supabase en production, Docker en local sur le port 5433).

Toute modification d'une route ou d'un champ commence par `docs/API.md`, puis back, puis front.

## Commandes

```bash
docker compose up -d db                       # Postgres local
cd api && cargo run -p fineme-server          # API sur :8080 (migrations au démarrage)
cd api && cargo test --workspace              # tests unitaires + intégration (Docker requis pour l'intégration)
cd api && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check
cd web && npm run dev                         # front sur :5173, proxy /api -> :8080
cd web && npm test                            # unitaires + intégration (Vitest)
cd web && npm run test:unit                   # fonctions pures seulement (test:integration : écrans + MSW)
cd web && npm run build && npm run lint
cd web && npm run e2e                         # Playwright, API et front démarrés
```

Rust : `export PATH=/opt/homebrew/opt/rustup/bin:$PATH` sur la machine de dev.

## Architecture back : hexagonale (ports et adaptateurs)

Workspace Cargo dans `api/crates/`. Le sens des dépendances est imposé par les `Cargo.toml` :
**les dépendances pointent toujours vers le domaine**, jamais l'inverse.

```
fineme-domain        entités, value objects, règles métier pures (aucune E/S, pas de sqlx/axum/reqwest/tokio)
   ^
fineme-application   cas d'usage + ports (traits) : repositories, PriceProvider, FxProvider, BankProvider, Clock, PasswordHasher, TokenService
   ^
fineme-infra         adaptateurs sortants : Postgres (sqlx), Yahoo/EODHD/CoinGecko/Frankfurter (reqwest), argon2, JWT
fineme-http          adaptateur entrant : routes Axum, DTO JSON, extraction d'auth, mapping des erreurs
   ^
fineme-server        binaire : configuration, composition root (câblage des adaptateurs), migrations
```

Règles :
- Le domaine ne connaît ni JSON ni SQL. Les DTO HTTP et les lignes SQL sont convertis aux frontières.
- Un cas d'usage = une intention métier ; il reçoit ses ports par injection (`Arc<dyn Port>` ou générique), jamais un pool SQL.
- Le temps passe par le port `Clock` (prêts, objectifs, instantanés) pour rester testable.
- Ajouter un agrégateur bancaire = un nouvel adaptateur implémentant `BankProvider`, sans toucher au domaine.
- Erreurs : `thiserror` par couche ; pas de `unwrap`/`expect`/`panic!` hors tests et hors démarrage ; les messages d'erreur renvoyés au client sont en français.
- Value objects pour les invariants (part de détention dans ]0,1], taux, identifiants typés) : un état invalide ne doit pas être constructible.
- Clippy sans avertissement, `cargo fmt`, pas de `unsafe`.
- Requêtes sqlx à l'exécution (`query`/`query_as`), pas les macros `query!` : le build ne dépend pas d'une base.

## Architecture front : par fonctionnalité

```
web/src/
  app/         routeur, providers, shell
  features/    <feature>/ { components, hooks, lib, pages }   (overview, accounts, real-estate, goals, projection, fees, connections, settings, auth)
  shared/      ui (composants génériques), api (client HTTP, types du contrat, routes, requêtes communes), lib (format, lecture des nombres, capitalisation), state
  test/        outillage des tests d'intégration : serveur MSW, fixtures fictives, rendu de l'application
web/e2e/       parcours Playwright
```

Les routes de l'API sont déclarées à un seul endroit (`shared/api/endpoints.ts`) ; chaque feature les appelle depuis ses
hooks. Les imports hors du dossier courant passent par l'alias `@/` ; oxlint (`no-restricted-imports`) refuse qu'une
feature importe une autre feature ou `app/`, et que `shared/` importe une feature.

Règles :
- Une feature n'importe jamais une autre feature ; ce qui est partagé va dans `shared/`.
- Composants = présentation ; les appels réseau vivent dans des hooks TanStack Query ; les calculs (projection, frais, prêt, FIRE) sont des fonctions pures dans `lib/`.
- TypeScript strict, pas de `any`, pas de valeur financière codée en dur dans l'UI : tout vient de l'API ou des fonctions pures.
- Chaque écran gère chargement, vide et erreur. Accessibilité : libellés de formulaires, navigation clavier, contrastes.
- Identité visuelle propre à FineMe (ne pas copier un produit existant).

## Principes

Clean code et SOLID appliqués sans cérémonie : petites fonctions nommées par intention, une responsabilité par
module, dépendre d'abstractions (ports) aux frontières uniquement. Patterns utilisés quand ils servent :
Repository, Strategy (fournisseurs de cours / agrégateurs), Adapter, Composition root, Value Object, Builder pour
les fixtures de test. Pas d'abstraction spéculative : une interface n'existe que si elle a deux implémentations
(la vraie et celle de test compte).

## Stratégie de test : pyramide

1. **Tests unitaires — large base.** Domaine et cas d'usage Rust (avec des faux ports en mémoire, sans base ni réseau) ;
   fonctions pures et hooks/composants isolés côté front (Vitest). Rapides, déterministes, la majorité des tests.
   Tout calcul financier a ses tests, y compris les cas limites (parts < 100 %, prêt non démarré, devise sans taux).
2. **Tests d'intégration — couche moyenne.** Adaptateurs Postgres contre un vrai Postgres (testcontainers) ;
   routes HTTP de bout en bout dans le processus (routeur Axum + base réelle) ; clients de cours contre un serveur
   HTTP simulé (wiremock). Côté front : écrans rendus avec React Testing Library et API simulée (MSW).
3. **Tests end-to-end — très peu.** Playwright sur les parcours critiques seulement (première configuration et
   connexion, création d'un compte et impact sur le patrimoine, simulateur). Aucune règle métier n'est validée
   uniquement en E2E.

Un bug corrigé = un test au niveau le plus bas capable de le reproduire.

## Sécurité et données

- Données financières réelles : rien de personnel dans Git (ni fichiers de seed, ni captures, ni fixtures). Les fixtures de test sont fictives.
- Secrets uniquement via variables d'environnement (`DATABASE_URL`, `JWT_SECRET`, `CORS_ORIGINS`, `CRON_SECRET`,
  `EODHD_API_KEY`) ; `.env.example` sans valeur réelle.
- Mots de passe hashés en argon2id ; toute route hors `/api/health` et `/api/auth/{status,setup,login,reset}` exige le JWT,
  sauf `/api/jobs/daily` (tâche planifiée) qui exige `CRON_SECRET` et refuse le JWT.
- Le JWT porte la version de session de l'utilisateur : changer ou réinitialiser le mot de passe l'incrémente et
  invalide tous les jetons antérieurs. La réinitialisation exige `RECOVERY_TOKEN` (code de secours côté serveur).

## Git

Messages de commit au format [Conventional Commits](https://www.conventionalcommits.org/fr/) : `type(portée): description`
à l'impératif, en minuscules, sans point final.

- Types : `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `chore`.
- Portées : `api`, `web`, `e2e`, `docs`, `deploy` (omise si le changement est transversal).
- Un commit = un changement cohérent ; un changement incompatible porte `!` et un pied `BREAKING CHANGE:`.
- Exemples : `feat(api): exiger un code à la première configuration`, `fix(web): corriger le total des parts`.

Une issue = une branche dédiée, créée depuis `main` avant de commencer : `<type>/<numéro>-<description-courte>`
(type de Conventional Commits, description en minuscules sans accents, mots séparés par des tirets), par exemple
`feat/21-rafraichissement-fiabilisation-cours` ou `fix/2-ci-startup-failure`. Les commits de la branche
citent l'issue en pied (`Refs #21`) et la pull request la ferme (`Closes #21`).

`make check` doit passer avant tout push.
