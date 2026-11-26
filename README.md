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
Décision : rester sur l'offre gratuite. Le front affiche « Réveil du serveur en cours » dès qu'une requête dépasse 5 s
et rejoue les erreurs passagères du réveil (réseau, 502/503) ; le workflow quotidien (ci-dessous) réveille déjà l'API
chaque matin. Si l'attente devient gênante : un ping périodique (ex. cron-job.org sur `/api/health` toutes les
10 minutes) ou l'offre payante de Render, qui ne met pas en veille.

### Cours et instantané quotidiens

Le workflow [`daily.yml`](.github/workflows/daily.yml) appelle chaque matin `POST /api/jobs/daily`, qui
rafraîchit les cours puis enregistre l'instantané de patrimoine du jour. À configurer une fois dans
*Settings > Secrets and variables > Actions* du dépôt : la **variable** `FINEME_API_URL` (URL Render de l'API) et
le **secret** `CRON_SECRET` (valeur de `CRON_SECRET` générée par Render, onglet *Environment*). Il se lance aussi
à la main depuis l'onglet *Actions* (*Run workflow*). Tout autre planificateur capable d'envoyer cette requête
convient (ex. cron-job.org).

Si un jour manque malgré tout (API injoignable, tâche en échec), la courbe d'évolution l'estime par interpolation
entre les deux instantanés qui l'encadrent et le trace en pointillés ; l'infobulle et une mention sous le graphique
le signalent.

Cours des titres : Yahoo Finance, puis **EODHD** en repli si `EODHD_API_KEY` est renseignée sur Render (clé
gratuite sur eodhd.com, nombre d'appels par jour limité ; le repli n'est appelé que pour les titres que Yahoo n'a
pas cotés). Un cours automatique qui n'a pas été mis à jour depuis plus de 4 jours est signalé « ancien » dans
la liste des positions.

## Brancher Claude sur FineMe

FineMe expose un serveur [MCP](https://modelcontextprotocol.io) **en lecture seule** sur `/mcp` : un assistant IA
(Claude Code, Claude Desktop) consulte le patrimoine avec six outils (`get_overview`, `get_performance`,
`get_budget`, `get_goals`, `compare_dates`, `list_alerts`) et ne peut rien modifier. Contrat : section
« Serveur MCP » de [docs/API.md](docs/API.md).

1. *Paramètres > Accès IA* : donner un nom au jeton, confirmer avec le mot de passe (et le code du second facteur
   s'il est activé), puis **copier le jeton** `fm_pat_…` : il n'est affiché qu'une fois. L'écran donne aussi la
   commande complète.
2. Dans un terminal :

   ```bash
   claude mcp add --transport http fineme http://localhost:8080/mcp --header "Authorization: Bearer fm_pat_…"
   ```

   Garder la portée par défaut : `--scope project` écrirait le jeton en clair dans `.mcp.json`, un fichier versionnable.

   En production, remplacer l'adresse par l'URL **https** de l'API (`https://<api>.onrender.com/mcp`) et renseigner
   `MCP_ALLOWED_HOSTS` sur Render avec l'hôte exact utilisé par le client (ex. `fineme-api.onrender.com`, domaine
   personnalisé compris). Par défaut seul `localhost` est accepté ; une fois la variable définie, elle remplace cette
   liste.
3. Dans Claude Code, `/mcp` liste les six outils. Exemple : « quel est mon patrimoine et sa variation sur 1 mois ? ».
4. `/rapport-patrimoine [1m|3m|6m|ytd|1y|all]` (skill du dépôt, `.claude/skills/rapport-patrimoine/SKILL.md`)
   rédige un rapport au format fixe : synthèse en trois lignes, patrimoine et variation (semaine et période),
   performance face à l'indice, budget face aux cibles, objectifs, alertes, points d'attention. Il n'utilise que
   les six outils et recopie leurs chiffres sans les recalculer.

Pour appeler chaque outil à la main : `npx @modelcontextprotocol/inspector`, transport *Streamable HTTP*, URL
`http://localhost:8080/mcp`, en-tête `Authorization: Bearer fm_pat_…`.

Sécurité :
- le jeton ne sert qu'à `/mcp` ; il n'ouvre aucune route `/api` (ni lecture, ni écriture). Le serveur n'en garde que
  l'empreinte SHA-256 ;
- *Paramètres > Accès IA* liste les jetons (date de dernière utilisation) et le journal des appels (outil, date,
  jeton) ; un jeton se révoque à tout moment, et **changer ou réinitialiser le mot de passe révoque tous les
  jetons** ;
- un jeton n'expire pas de lui-même : révoquer ceux qui ne servent plus ;
- `claude mcp add --header` enregistre le jeton en clair dans la configuration locale de Claude Code
  (`~/.claude.json`) : le traiter comme un mot de passe, et le révoquer si la machine est perdue ;
- les libellés de transactions et les noms importés sont présentés à l'assistant comme des données, jamais comme
  des instructions.

## Rapport hebdomadaire par email

Chaque lundi, le workflow `.github/workflows/weekly-report.yml` lance un agent
([Claude Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview), `automation/weekly-report/`) qui lit le serveur
MCP de production et envoie le rapport au même format que `/rapport-patrimoine`. L'agent n'a que les six outils FineMe :
ni terminal, ni fichiers, ni web. Si l'API ne répond pas (réveil de Render compris), si le jeton est refusé ou si le
rapport est mal formé, un email « Rapport de patrimoine indisponible » part et l'exécution apparaît en échec.

1. Sur Render, `MCP_ALLOWED_HOSTS` doit contenir l'hôte public de l'API (voir ci-dessus).
2. Dans FineMe, *Paramètres > Accès IA* : créer un jeton dédié (« Rapport hebdo »), révocable à part.
3. Dans un terminal, `claude setup-token` produit le jeton de l'abonnement Claude utilisé par l'agent.
4. Dans GitHub, *Settings > Secrets and variables > Actions* :

   | Type | Nom | Valeur |
   |---|---|---|
   | variable | `FINEME_API_URL` | URL https de l'API (déjà définie pour la mise à jour quotidienne) |
   | secret | `FINEME_MCP_TOKEN` | jeton `fm_pat_…` de l'étape 2 |
   | secret | `CLAUDE_CODE_OAUTH_TOKEN` | jeton de l'étape 3 |
   | secret | `SMTP_HOST`, `SMTP_PORT` | serveur d'envoi, ex. `smtp.gmail.com` et `587` (TLS obligatoire) |
   | secret | `SMTP_USER`, `SMTP_PASSWORD` | compte d'envoi ; Gmail : un [mot de passe d'application](https://myaccount.google.com/apppasswords) |
   | secret | `REPORT_EMAIL_TO` | destinataire |
   | secret (facultatif) | `REPORT_EMAIL_FROM` | expéditeur, par défaut `SMTP_USER` |
   | variable (facultatif) | `REPORT_MODEL` | modèle, par défaut `claude-opus-5-5` |

   Préférer un **compte d'envoi dédié**, sans autre usage : un mot de passe d'application Gmail ouvre aussi la lecture
   de la boîte (IMAP, POP). Le révoquer au moindre doute.

5. *Actions > Rapport hebdomadaire > Run workflow* pour un premier envoi.

Un libellé importé ne doit pas pouvoir faire écrire un lien cliquable : dans l'email, les adresses avec schéma, les
adresses email et les liens `mailto:`, `tel:`, `sms:` sont remplacés par « [lien retiré] », et les domaines ou adresses
IP seuls sont désamorcés (`example[.]com`).

À savoir : le rapport contient des données financières et circule en email (texte brut) ; il passe aussi par l'API
d'Anthropic. Le dépôt étant public, l'agent n'écrit dans les journaux que des messages fixes, jamais le rapport ; la variable
`FINEME_API_URL` y reste visible (GitHub ne masque que les secrets), comme pour la mise à jour quotidienne. GitHub
suspend les workflows planifiés d'un dépôt public après 60 jours sans activité (réactivables depuis l'onglet Actions).

En local (Node 24 ou plus) : copier `automation/weekly-report/.env.example` en `.env`, puis
`cd automation/weekly-report && npm install && node --env-file=.env src/main.ts`.

## Connexions bancaires

Aucun agrégateur n'est branché. L'application fonctionne en saisie manuelle et import CSV ; ajouter un agrégateur
revient à implémenter le port `BankProvider` (`api/crates/fineme-application/src/ports.rs`) dans `fineme-infra`.
