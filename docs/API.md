# FineMe — contrat d'API (source de vérité front ↔ back)

Application personnelle de suivi de patrimoine (un seul login, plusieurs membres du foyer).

- `api/` : Rust (Axum + sqlx + Postgres), hébergé sur Render, base Postgres Supabase.
- `web/` : React + Vite + TypeScript, hébergé sur Vercel.
- Toutes les routes sont préfixées par `/api`. JSON partout, `snake_case`.
- Auth : `Authorization: Bearer <JWT>` sur toutes les routes sauf `/api/health`, `/api/auth/status`, `/api/auth/setup`, `/api/auth/login`, `/api/auth/reset`.
  `/api/jobs/daily` n'accepte pas le JWT : elle exige `Authorization: Bearer <CRON_SECRET>` (voir « Tâche quotidienne »).
- Montants : nombres JSON (f64). Devise de référence : EUR. Dates : `YYYY-MM-DD`. Identifiants : UUID.
- Parts de détention : fraction `0..1` (0.5 = 50 %).
- Erreurs : statut HTTP + `{ "error": "message lisible en français" }`.
- CORS : origines autorisées via `CORS_ORIGINS` (liste séparée par des virgules).

## Variables d'environnement (api)

`DATABASE_URL`, `JWT_SECRET`, `CORS_ORIGINS`, `PORT` (défaut 8080), `SETUP_TOKEN` (optionnel en local, à définir en production : code exigé pour la première configuration, afin que personne d'autre ne puisse créer le compte), `RECOVERY_TOKEN` (optionnel : code de secours qui permet de réinitialiser un mot de passe perdu ; absent = récupération désactivée ; au moins 16 caractères, sinon le serveur refuse de démarrer), `CRON_SECRET` (optionnel : secret de la tâche quotidienne `/api/jobs/daily` ; absent = tâche désactivée ; au moins 16 caractères, sinon le serveur refuse de démarrer), `EODHD_API_KEY` (optionnel : clé du fournisseur de cours de repli EODHD ; absente = pas de repli pour les titres), `TRUSTED_PROXY_HOPS` (optionnel, défaut 1 : nombre de proxys de confiance devant l'API, voir « Limitation des tentatives »).
Les migrations sqlx s'exécutent au démarrage.

## Auth

| Méthode | Route | Corps | Réponse |
|---|---|---|---|
| GET | `/api/health` | – | `{ "status": "ok" }` |
| GET | `/api/auth/status` | – | `{ "setup_required": bool, "setup_token_required": bool, "recovery_enabled": bool }` (`setup_required` true tant qu'aucun utilisateur n'existe ; `setup_token_required` true si `SETUP_TOKEN` est défini côté serveur) |
| POST | `/api/auth/setup` | `{ email, password, name, setup_token? }` | `{ token, user }` — refusé (409) si un utilisateur existe déjà ; refusé (403) si `SETUP_TOKEN` est défini et que `setup_token` est absent ou différent (comparaison en temps constant). Crée aussi le membre principal `name`. |
| POST | `/api/auth/login` | `{ email, password }` | `{ token, user }` |
| GET | `/api/auth/me` | – | `user` = `{ id, email, name }` |
| PUT | `/api/auth/password` | `{ current_password, new_password }` | `{ token, user }` — refusé (403) si `current_password` est faux ; (400) si `new_password` est trop court ou identique à l'actuel. Le jeton renvoyé remplace celui de l'appareil courant. |
| POST | `/api/auth/reset` | `{ email, recovery_token, new_password }` | `{ token, user }` — (501) si `RECOVERY_TOKEN` n'est pas défini ; (403) si `recovery_token` est absent ou différent (comparaison en temps constant, vérifiée avant tout le reste) ; (400) mot de passe trop court ; (404) aucun compte pour cet e-mail. |

| POST | `/api/auth/refresh` | – | `{ token, user }` — renouvellement glissant : nouveau jeton valable 7 jours, à la version de session courante. |
| GET | `/api/auth/logins?limit=` | – | `LoginEvent[]`, du plus récent au plus ancien (`limit` défaut 50, borné à 1..200). |

Mot de passe ≥ 10 caractères, hashé en argon2id. JWT valable **7 jours** ; le front appelle `/api/auth/refresh`
au chargement puis toutes les heures dès que le jeton a plus d'un jour : une session active n'expire pas,
un appareil oublié est déconnecté au bout de 7 jours d'inactivité.

### Limitation des tentatives

`/api/auth/login` et `/api/auth/reset` partagent un limiteur en mémoire. Ne comptent comme échecs que les refus
d'identité (401, 403, 404) ; une saisie invalide (400) ne compte pas.

- Par adresse IP : 5 échecs tolérés, puis un délai de 30 s, doublé à chaque nouvel échec (1 min, 2 min…), plafonné à 15 min.
  Une connexion réussie remet le compteur de l'IP à zéro ; sans échec pendant 1 h, il est oublié.
- Global (toutes IP) : 50 échecs tolérés, puis même progression plafonnée à 5 min.
- Pendant le délai, la route répond **429** `{ "error": "Trop de tentatives. Réessayez dans 30 secondes." }` avec
  l'en-tête `Retry-After: <secondes>` (exposé en CORS), sans vérifier le mot de passe.

IP du client : derrière le proxy de Render, elle est lue dans `X-Forwarded-For`, à la position
`TRUSTED_PROXY_HOPS` **depuis la droite** (défaut 1 : la dernière entrée, ajoutée par le proxy ; les entrées de
gauche peuvent être forgées par le client et sont ignorées). `TRUSTED_PROXY_HOPS=0` ignore l'en-tête et
utilise l'adresse TCP (sans proxy). Sans IP connue, toutes les requêtes partagent un même compteur.

### Journal des connexions

`LoginEvent = { at: string (RFC 3339), action: "login" | "reset", success: bool, ip: string | null, user_agent: string | null }`

Chaque tentative de connexion ou de réinitialisation qui atteint la vérification est journalisée (pas les refus
429 ni les saisies invalides). Le user-agent est tronqué à 200 caractères ; l'e-mail saisi n'est pas conservé.
Les entrées de plus de 90 jours sont purgées à chaque connexion réussie.

`GET /api/auth/status` renvoie aussi `recovery_enabled: bool` (true si `RECOVERY_TOKEN` est défini), pour
afficher ou non le lien « Mot de passe oublié ».

Sessions : chaque utilisateur porte un numéro de version de session, recopié dans le JWT. Changer ou
réinitialiser le mot de passe l'incrémente : tous les jetons émis auparavant sont refusés (401), sur tous les
appareils. Seul le jeton renvoyé par la route reste valable.

## Membres du foyer

`Member = { id, name, color, is_primary }`

- GET `/api/members` → `Member[]`
- POST `/api/members` `{ name, color? }` → `Member`
- PUT `/api/members/:id` `{ name?, color? }` → `Member`
- DELETE `/api/members/:id` → 204 (refusé 409 si membre principal ou encore propriétaire d'un compte)

### Filtre de vue

Presque toutes les routes de lecture acceptent `?member_id=<uuid>`. Absent = vue foyer
(somme des parts de tous les membres). Présent = part de ce membre uniquement.
`owned_value` = `value` × part(s) concernée(s).

## Comptes

```ts
type AccountKind = "checking" | "savings" | "investment" | "crypto" | "real_estate" | "loan" | "other";
// enveloppe fiscale / type de contrat, libre mais valeurs connues :
type Envelope = "pea" | "pea_pme" | "cto" | "assurance_vie" | "per" | "livret" | "pee" | null;

type Account = {
  id: string;
  name: string;
  institution: string | null;
  kind: AccountKind;
  envelope: Envelope;
  currency: string;            // ISO, défaut "EUR"
  cash_balance: number;        // solde espèces dans la devise du compte
  annual_yield_pct: number | null; // ex. 1.7 pour un livret
  source: "manual" | "csv" | "aggregator" | "import";
  external_id: string | null;
  connection_id: string | null;
  opened_at: string | null;
  owners: { member_id: string; name: string; share: number }[];
  // calculés :
  value: number;               // valeur totale en EUR (positive, y compris pour un prêt = capital restant dû)
  owned_value: number;         // selon le filtre de vue
  is_liability: boolean;       // true pour kind = "loan"
  invested: number | null;     // prix de revient total en EUR si connu
  gain: number | null;         // value - invested
  updated_at: string;          // ISO datetime
};
```

Calcul de `value` :
- `real_estate` : `property.current_value`
- `loan` : capital restant dû à aujourd'hui (tableau d'amortissement ; avant `start_date` = `principal`)
- autres : `cash_balance` + Σ `holding.quantity × holding.current_price`, convertis en EUR.

Routes :
- GET `/api/accounts` → `Account[]`
- POST `/api/accounts` `{ name, institution?, kind, envelope?, currency?, cash_balance?, annual_yield_pct?, opened_at?, owners: [{member_id, share}], property?: PropertyInput, loan?: LoanInput }` → `AccountDetail`
  (`property` requis si `kind = real_estate`, `loan` requis si `kind = loan`)
- GET `/api/accounts/:id` → `AccountDetail` = `Account & { holdings: Holding[], property: Property | null, loan: Loan | null }`
- PUT `/api/accounts/:id` (mêmes champs, tous optionnels ; `owners` remplace la liste) → `AccountDetail`
- DELETE `/api/accounts/:id` → 204

## Positions (holdings)

```ts
type Holding = {
  id: string; account_id: string;
  kind: "security" | "crypto" | "fonds_euro" | "other";
  name: string;
  isin: string | null;
  symbol: string | null;       // ticker ou code crypto (BTC)
  security_type: string | null; // etf | stock | fund | bond | scpi ...
  quantity: number;
  buy_price: number | null;    // PRU unitaire, devise `currency`
  current_price: number;       // devise `currency`
  currency: string;
  expense_ratio: number | null; // frais courants annuels, fraction (0.002 = 0,2 %)
  price_source: "manual" | "auto";
  price_updated_at: string | null;
  price_stale: boolean;        // true si le cours est automatique et date de plus de 4 jours (ou n'a pas de date)
  // calculés (EUR) :
  value: number; invested: number | null; gain: number | null; gain_pct: number | null;
};
```

- POST `/api/accounts/:id/holdings` `HoldingInput` → `Holding`
- POST `/api/accounts/:id/holdings/bulk` `{ holdings: HoldingInput[], replace: bool }` → `Holding[]` (import CSV)
- PUT `/api/holdings/:id` → `Holding`
- DELETE `/api/holdings/:id` → 204
- POST `/api/prices/refresh` → `{ updated: number, failed: { holding_id, name, reason }[] }`
  Best effort : crypto via CoinGecko (par `symbol`), titres via recherche ISIN/ticker sur Yahoo Finance,
  taux de change via frankfurter.app. Un titre que Yahoo ne cote pas est redemandé à EODHD (recherche par
  ISIN ou ticker), si `EODHD_API_KEY` est défini. Un échec ne casse jamais la requête ; le prix existant reste.
  `reason` réunit les raisons de chaque fournisseur essayé, séparées par « ; ».
  Chaque cours récupéré est aussi enregistré dans l'historique des cours (un point par position et par jour,
  le dernier du jour l'emporte).
- GET `/api/holdings/:id/prices?range=1m|3m|1y|all` → `{ points: [{ date, price, currency }] }` (du plus ancien
  au plus récent ; cours récupérés automatiquement uniquement ; 404 si la position n'existe pas)
- GET `/api/fx` → `{ base: "EUR", rates: { "USD": 1.10, ... }, updated_at }` (rates = unités de devise pour 1 EUR)

## Transactions (import CSV / agrégateur)

`Transaction = { id, account_id, date, label, amount, category: string | null, manual_category: string | null, internal_transfer: boolean | null }`
(amount signé, devise du compte ; `category` = catégorie reçue à l'import ; `manual_category` et `internal_transfer` =
choix manuels du budget, `null` = automatique, voir « Budget »)

- GET `/api/accounts/:id/transactions?limit=&offset=` → `{ items: Transaction[], total: number }`
- POST `/api/accounts/:id/transactions/bulk` `{ transactions: [{date,label,amount,category?}], new_cash_balance?: number }` → `{ inserted, skipped }`
  (dédoublonnage sur date+label+amount)
- PUT `/api/transactions/:id` `{ category?: string | null, internal_transfer?: boolean | null }` → `Transaction`
  (`category` : catégorie manuelle, qui crée ou met à jour la règle du libellé, voir « Budget » ; `null` la retire.
  `internal_transfer` : `true`/`false` force le marquage de virement interne, `null` revient à la détection automatique.
  Un champ absent n'est pas modifié.)
- DELETE `/api/transactions/:id` → 204

Le parsing du CSV et le mapping des colonnes se font côté front.

## Journal d'opérations (comptes d'investissement)

Achats, ventes, dividendes, frais, versements et retraits d'un compte pouvant porter des positions
(tous sauf `real_estate` et `loan` : (400) sinon).

```ts
type OperationKind = "buy" | "sell" | "dividend" | "fee" | "deposit" | "withdrawal";
type Operation = {
  id: string; account_id: string;
  kind: OperationKind;
  date: string;                 // YYYY-MM-DD, jamais dans le futur
  holding_id: string | null;    // requis pour buy/sell, facultatif pour dividend/fee, null pour deposit/withdrawal
  holding_name: string | null;
  quantity: number | null;      // buy/sell uniquement, > 0
  unit_price: number | null;    // buy/sell uniquement, cours unitaire ≥ 0
  amount: number;               // buy/sell : quantity × unit_price (brut, hors frais) ; sinon montant saisi > 0
  fees: number;                 // frais de l'achat ou de la vente (≥ 0) ; 0 pour les autres types
  currency: string;             // devise de la position concernée, sinon du compte
  note: string | null;
  realized_gain: number | null; // vente uniquement : plus-value réalisée nette des frais, devise `currency`
};
type OperationInput = {
  kind: OperationKind; date: string; holding_id?: string;
  quantity?: number; unit_price?: number; fees?: number; // buy/sell
  amount?: number;                                       // dividend/fee/deposit/withdrawal
  note?: string;
};
```

- GET `/api/accounts/:id/operations` → `Operation[]` (du plus récent au plus ancien)
- POST `/api/accounts/:id/operations` `OperationInput` → `Operation` (201)
- PUT `/api/operations/:id` `OperationInput` (remplacement complet, même compte) → `Operation`
- DELETE `/api/operations/:id` → 204
- GET `/api/accounts/:id/operations/summary` →
  `{ currency: "EUR", years: { year, realized_gain, dividends, fees, deposits, withdrawals }[] }` (année la plus récente
  d'abord ; `fees` = frais des achats et ventes + opérations `fee` ; montants convertis en EUR aux taux actuels ;
  compte entier, sans filtre de membre)

Erreurs : (400) champ manquant ou invalide pour le type, date future, vente supérieure à la quantité détenue à sa date
(y compris quand une modification ou une suppression rendrait une vente ultérieure impossible : rien n'est modifié) ;
(404) compte, opération ou position introuvable (la position doit appartenir au compte).

Calcul (PRU moyen pondéré, méthode française) : le journal est rejoué par ordre chronologique (date, puis ordre de
saisie). Un achat recalcule le PRU = (quantité × PRU + quantité achetée × cours + frais) / nouvelle quantité. Une
vente ne change pas le PRU et réalise `quantity × (unit_price − PRU) − fees`. Une vente totale remet la position à
zéro (`buy_price` = null) ; le PRU repart du prochain achat. Dividendes, frais, versements et retraits ne changent
ni la quantité ni le PRU.

Cohabitation avec la saisie manuelle :
- une position **sans achat ni vente** dans le journal garde sa quantité et son `buy_price` saisis à la main ;
- dès son premier achat ou sa première vente, sa quantité et son `buy_price` sont **entièrement** recalculés à partir
  du journal à chaque modification de celui-ci (la saisie manuelle antérieure est remplacée : saisir la position
  initiale comme un achat). `PUT /api/holdings/:id` refuse alors (409) de changer `quantity` ou `buy_price`
  (renvoyer les valeurs actuelles reste accepté) ; le cours et les autres champs restent modifiables ;
- supprimer la dernière opération d'achat/vente d'une position la remet à zéro ;
- le journal ne modifie jamais `cash_balance` du compte ;
- supprimer une position supprime son journal ; l'export/import complet (`Bundle`) ne contient pas le journal.

## Immobilier et prêts

```ts
type Property = {
  id: string; account_id: string;
  address: string | null;
  category: "primary" | "secondary" | "rental";
  building_type: "house" | "apartment" | "land" | "parking" | "other";
  surface_m2: number | null;
  purchase_price: number; purchase_fees: number; // frais de notaire/agence
  purchased_at: string | null;
  current_value: number;
  monthly_rent: number;          // 0 si non loué
  monthly_charges: number;       // charges non récupérables
  yearly_property_tax: number;
};
type Loan = {
  id: string; account_id: string;
  property_account_id: string | null; // compte immobilier financé
  principal: number;
  interest_rate_pct: number;  // nominal annuel, ex. 4.0
  insurance_rate_pct: number; // annuel sur capital initial, ex. 0.25
  duration_months: number;
  start_date: string;
  // calculés :
  monthly_payment: number;    // hors assurance
  monthly_insurance: number;
  remaining_capital: number;
  end_date: string;
  total_interest: number; total_insurance: number;
};
```

- GET `/api/real-estate` → `{ properties: RealEstateItem[], totals: { value, remaining_debt, equity, monthly_rent, monthly_payments } }`
  `RealEstateItem = { account: Account, property: Property, loans: (Loan & {account_id, name})[], equity, gross_yield_pct | null, net_yield_pct | null, latent_gain }`
  (`equity` = valeur − capital restant dû des prêts liés ; rendements seulement si `monthly_rent > 0`)
- GET `/api/loans/:account_id/schedule` → `{ loan: Loan, rows: [{ month: number, date, payment, interest, principal, insurance, remaining }] }`

## Vue d'ensemble

GET `/api/overview` →
```ts
{
  gross_assets: number; liabilities: number; net_worth: number;
  financial_assets: number;   // épargne + titres + crypto (hors comptes courants, immobilier, dettes)
  invested: number; gain: number; // sur les positions dont le PRU est connu
  allocation: { category: "checking"|"savings"|"investment"|"crypto"|"real_estate"|"other"; label: string; value: number; pct: number }[];
  by_institution: { institution: string; value: number }[];
  by_envelope: { envelope: string; value: number }[];
  members: { member_id: string; name: string; color: string; gross_assets: number; liabilities: number; net_worth: number }[];
  top_holdings: { name: string; symbol: string | null; value: number; gain_pct: number | null; account_name: string }[];
  weighted_expense_ratio: number | null; // frais courants moyens pondérés des positions (fraction)
}
```
GET `/api/history?range=1m|3m|1y|all` → `{ points: HistoryPoint[] }` (du plus ancien au plus récent)
```ts
type HistoryPoint = {
  date: string; gross_assets: number; liabilities: number; net_worth: number;
  // Détail par compte ; null pour les instantanés enregistrés avant l'introduction du détail.
  accounts: HistoryAccount[] | null;
};
type HistoryAccount = {
  account_id: string;   // le compte a pu être supprimé depuis : l'historique le garde
  name: string;         // nom du compte au jour de l'instantané
  kind: AccountKind;    // catégorie du compte ; "loan" = dette
  value: number;        // valeur détenue dans la vue, en EUR (positive, y compris pour un prêt)
  gain: number | null;  // plus-value latente détenue des positions au PRU connu ; null si aucune
};
```

Un instantané du jour (foyer + chaque membre) est enregistré/mis à jour à chaque appel de `/api/overview`
et par la tâche quotidienne. `/api/history` respecte `member_id`.

Détail : seuls les comptes dont la vue détient une part figurent dans `accounts` (tous pour le foyer).
Σ `value` des comptes hors prêts = `gross_assets`, Σ `value` des prêts = `liabilities`, Σ `gain` = `gain` de
`/api/overview` au même jour. La répartition par catégorie se déduit de `kind` (mêmes catégories que `allocation`).
Une période personnalisée se filtre côté front dans la réponse `range=all`.

### Nettoyage de l'historique

Routes protégées par le JWT, pour corriger les graphiques (point aberrant, journée en échec, données de test).

GET `/api/history/points?member_id=&threshold=` → `HistoryReview` (tous les points de la vue, du plus ancien au plus récent)
```ts
type HistoryReview = {
  threshold: number;          // seuil appliqué, fraction (0.2 = 20 %) ; défaut 0.2, accepté dans ]0, 10]
  points: HistoryReviewPoint[];
};
type HistoryReviewPoint = {
  date: string; gross_assets: number; liabilities: number; net_worth: number;
  detailed: boolean;          // false : pas de détail par compte (instantané ancien ou corrigé)
  corrected: boolean;         // valeur corrigée à la main
  change: number | null;      // variation du patrimoine net depuis le point précédent, fraction de la base
  suspicious: boolean;        // variation anormale (voir ci-dessous)
};
```
Point suspect : `change` = (net − net précédent) / base, où base = max(|net précédent|, actifs bruts précédents).
Un point est suspect si |`change`| > `threshold` **et** si l'écart absolu atteint au moins 100 €. Un point qui
ramène la courbe au niveau d'avant un point suspect (retour à la normale, |variation depuis l'avant-dernier
point| ≤ seuil) n'est pas signalé : seul le pic l'est. Le premier point, un point dont la base est nulle et un point
corrigé à la main ne sont jamais suspects. Les jours sans instantané ne comptent pas : on compare deux points
consécutifs. (400) seuil invalide ; (404) membre inconnu.

PUT `/api/history/points/:date?member_id=` `{ gross_assets, liabilities }` → `HistoryReviewPoint`
Corrige le point de ce jour pour la vue demandée seulement (foyer par défaut). `net_worth` = `gross_assets` −
`liabilities` ; les deux montants sont finis et ≥ 0. Le détail par compte du point est retiré (il ne
correspondrait plus aux totaux : `accounts` devient `null` dans `/api/history`) et le point est marqué `corrected`.
(400) montant invalide, ou date du jour : le point du jour est recalculé à chaque instantané, il se corrige en
modifiant les comptes puis en le recalculant ; (404) aucun point ce jour-là pour cette vue, ou membre inconnu.

DELETE `/api/history/points?from=YYYY-MM-DD&to=YYYY-MM-DD` → `{ deleted_days: number }`
Supprime les instantanés de ces jours (bornes incluses ; `from = to` pour un seul point) **pour toutes les vues**
(foyer et chaque membre, relevés ensemble chaque jour). (400) date absente ou illisible, `from` après `to`. Le point du
jour supprimé réapparaît au prochain instantané (vue d'ensemble ou tâche quotidienne).

POST `/api/history/recompute` (sans corps) → `{ snapshot_date }`
Reprend l'instantané du jour (foyer et chaque membre) à partir des données actuelles, comme la tâche quotidienne
sans rafraîchir les cours. Limite : les jours passés ne peuvent pas
être recalculés, car l'historique des soldes, des valeurs de biens et des positions n'est pas conservé (seuls les
cours automatiques le sont) ; ils se corrigent ou se suppriment.

POST `/api/history/reset` `{ confirmation: "EFFACER" }` → `{ deleted_days: number }`
Supprime tout l'historique, toutes vues confondues. Action protégée : (400) si `confirmation` n'est pas exactement
`EFFACER` (espaces de début et de fin ignorés), rien n'est supprimé.

## Budget

Revenus, dépenses et taux d'épargne calculés à partir des transactions de tous les comptes, convertis en EUR et
multipliés par la part de la vue (`?member_id=` comme ailleurs : un membre ne voit que sa part des comptes qu'il détient).

**Libellé normalisé** : minuscules, accents retirés, chiffres retirés, ponctuation remplacée par des espaces, espaces
réduits (`"PRLV SEPA EDF 0423/11"` → `"prlv sepa edf"`).

**Catégorie effective** d'une transaction, par ordre de priorité :
1. `manual_category` (choisie dans le budget) — source `"manual"` ;
2. règle utilisateur dont le motif est contenu dans le libellé normalisé (le motif le plus long l'emporte) — `"rule"` ;
3. `category` reçue à l'import — `"import"` ;
4. règle par défaut (mots-clés génériques : salaire, loyer, supermarchés, carburant…) — `"default"` ;
5. sinon aucune (`null`, affichée « Non catégorisé »).

Recatégoriser une transaction (`PUT /api/transactions/:id` avec `category`) crée ou met à jour la règle dont le motif est
son libellé normalisé : les transactions de même libellé, passées et suivantes, prennent cette catégorie (sauf choix
manuel propre).

**Virements internes** (exclus des revenus et des dépenses) : deux transactions de **comptes différents** du foyer,
de montants opposés (écart < 0,01) et datées à **3 jours ou moins** d'écart, sont appariées automatiquement (chaque
transaction au plus une fois, l'écart de dates le plus court d'abord). `internal_transfer` sur la transaction force le
marquage dans un sens ou dans l'autre ; une transaction marquée à la main n'est plus appariée automatiquement (marquer
chaque jambe si besoin).

**Calculs du mois** : revenus = Σ montants positifs, dépenses = Σ |montants négatifs| (hors virements internes),
épargne = revenus − dépenses, taux d'épargne = épargne / revenus (`null` si aucun revenu).

**Charges récurrentes** : dépenses de même libellé normalisé présentes sur au moins 3 mois distincts, d'intervalle
moyen entre 25 et 35 jours et de montants à ±15 % de leur médiane (analyse des 12 derniers mois).

```ts
type BudgetCategorySource = "manual" | "rule" | "import" | "default";
type BudgetFlows = { month: string /* YYYY-MM */; income: number; expenses: number; savings: number; savings_rate: number | null };
type BudgetMonth = BudgetFlows & {
  categories: { category: string | null; amount: number; target: number | null }[]; // dépenses du mois, décroissant ;
                                    // les catégories avec cible mais sans dépense apparaissent avec amount = 0
  trend: BudgetFlows[];             // 12 mois, du plus ancien au mois demandé inclus
  recurring: { label: string; category: string | null; average_amount: number; occurrences: number;
               last_date: string; next_date: string }[]; // average_amount positif (EUR, part de la vue)
  internal_transfers: number;       // nombre de transactions du mois exclues comme virements internes
};
type BudgetTransaction = {
  id: string; account_id: string; account_name: string; date: string; label: string;
  amount: number;                   // signé, devise du compte
  amount_eur: number;               // signé, EUR × part de la vue
  category: string | null; category_source: BudgetCategorySource | null;
  internal_transfer: boolean; internal_transfer_manual: boolean;
};
type BudgetRule = { id: string; pattern: string; category: string };
type BudgetTarget = { category: string; monthly_amount: number };
```

- GET `/api/budget?month=YYYY-MM&member_id=` → `BudgetMonth` (`month` absent = mois courant ; 400 si mal formé)
- GET `/api/budget/transactions?month=YYYY-MM&member_id=` → `BudgetTransaction[]` (du plus récent au plus ancien)
- GET `/api/budget/categories` → `string[]` (catégories par défaut, des règles, des cibles et des transactions, triées)
- GET `/api/budget/rules` → `BudgetRule[]` ; DELETE `/api/budget/rules/:id` → 204
- GET `/api/budget/targets` → `BudgetTarget[]`
- PUT `/api/budget/targets` `{ category, monthly_amount: number | null }` → `BudgetTarget[]` (crée ou remplace la cible
  mensuelle de la catégorie ; `null` la supprime ; montant ≥ 0)

## Tâche quotidienne

POST `/api/jobs/daily`, en-tête `Authorization: Bearer <CRON_SECRET>`, sans corps →
`{ prices: { updated, failed: { holding_id, name, reason }[] }, snapshot_date: "YYYY-MM-DD", alerts: AlertEvaluation }`

Rafraîchit les taux et les cours (comme `/api/prices/refresh`), puis enregistre l'instantané du jour (foyer + chaque
membre), et évalue enfin les alertes (voir « Alertes et rappels ») sur cet instantané. Appelée chaque matin par un planificateur externe (workflow GitHub Actions `daily.yml`). Idempotente :
un second appel le même jour remplace les cours et l'instantané du jour.
- (501) si `CRON_SECRET` n'est pas défini ; (403) si le secret est absent ou différent (comparaison en temps constant).

## Alertes et rappels

```ts
type AlertRuleKind = "goals" | "savings_ceiling" | "net_worth_change" | "connections" | "stale_prices";
type AlertRule = {
  kind: AlertRuleKind;
  enabled: boolean;
  threshold_pct: number | null; // savings_ceiling : ]0, 100] (défaut 90) ; net_worth_change : ]0, 1000] (défaut 10) ; sinon null
  ceiling: number | null;       // savings_ceiling seulement : plafond en EUR (défaut 22 950, plafond du Livret A)
};
type Alert = {
  id: string;
  kind: "goal_reached" | "goal_overdue" | "savings_ceiling" | "net_worth_change"
      | "connection_expired" | "connection_error" | "stale_prices";
  severity: "info" | "warning";
  title: string; message: string; // en français, prêts à afficher
  created_at: string;             // ISO datetime (première détection, ou réapparition)
  read: boolean; archived: boolean;
  active: boolean;                // false : la condition a disparu depuis (alerte résolue)
};
type AlertEvaluation = { created: number; reopened: number; resolved: number };
```

Règles (toutes activées par défaut ; une règle désactivée n'est pas évaluée) :
- `goals` : objectif atteint (`completed` ou `funded`) → `goal_reached` ; date cible dépassée (`overdue`) → `goal_overdue`.
  Le statut est celui de `GET /api/goals`.
- `savings_ceiling` : compte `savings` dont la valeur (EUR) atteint `threshold_pct` % de `ceiling` (ou le dépasse).
- `net_worth_change` : |variation| du patrimoine net du foyer entre les deux derniers instantanés des 31 derniers jours
  ≥ `threshold_pct` %, rapportée à |patrimoine précédent| (aucune alerte si le précédent vaut 0).
- `connections` : connexion bancaire `expired` → `connection_expired`, `error` → `connection_error`.
- `stale_prices` : une seule alerte tant qu'au moins une position a un cours automatique ancien (`price_stale`).

Dédoublonnage : chaque condition a une clé stable (règle + objet concerné, + date du dernier instantané pour
`net_worth_change`). Tant qu'elle dure, l'évaluation garde la même alerte (son état lu / archivé est conservé, son texte
mis à jour) ; quand elle disparaît, l'alerte passe `active: false` ; si elle réapparaît, l'alerte est rouverte (non lue,
désarchivée, `created_at` = date de réapparition). Une évaluation quotidienne ne crée donc pas une alerte par jour.

Routes :
- GET `/api/alerts?archived=true|false` (défaut `false`) → `Alert[]`, de la plus récente à la plus ancienne
- POST `/api/alerts/evaluate` → `AlertEvaluation` (évaluation à la demande ; aussi faite par la tâche quotidienne)
- PUT `/api/alerts/:id` `{ read?: boolean, archived?: boolean }` → `Alert` (404 si inconnue)
- POST `/api/alerts/read-all` → `{ updated: number }` (marque lues les alertes non archivées)
- GET `/api/alerts/rules` → `AlertRule[]` (une par type, valeurs par défaut si jamais modifiée)
- PUT `/api/alerts/rules/:kind` `{ enabled?, threshold_pct?, ceiling? }` → `AlertRule` (400 si seuil invalide ou non
  applicable à la règle ; 404 si `kind` inconnu)

Non fait : résumé périodique par e-mail (exige un service d'envoi externe).

## Objectifs

```ts
type Goal = {
  id: string; name: string; icon: string | null;
  kind: "safety_net" | "project";
  // project :
  target_amount: number | null; target_date: string | null; monthly_contribution: number | null;
  expected_return_pct: number | null; // hypothèse de rendement annuel, ex. 7
  // safety_net :
  monthly_expenses: number | null; coverage_months: number | null;
  account_ids: string[]; member_ids: string[];
  progress: {
    current_amount: number; target_amount: number; pct: number;
    status: "completed" | "on_track" | "challenging" | "overdue" | "underfunded" | "funded";
    required_return_pct: number | null; // rendement annuel nécessaire pour atteindre la cible à la date
    projected_amount_at_date: number | null; // avec expected_return_pct et monthly_contribution
    coverage_months: number | null; // safety_net
  };
};
```
`current_amount` = Σ valeur détenue par le foyer des comptes liés.
- GET `/api/goals`, POST `/api/goals`, PUT `/api/goals/:id`, DELETE `/api/goals/:id`

## Connexions bancaires (agrégateur)

Architecture prête, aucun fournisseur activé par défaut (trait `BankProvider` côté Rust).

- GET `/api/connections/providers` → `{ id, name, configured: bool }[]` (liste vide si aucun fournisseur compilé/configuré)
- GET `/api/connections` → `{ id, provider, institution, status: "active"|"error"|"expired", last_sync_at, error }[]`
- POST `/api/connections` `{ provider }` → `{ id, redirect_url }` (501 si fournisseur non configuré)
- POST `/api/connections/:id/sync` → `{ accounts_updated, transactions_inserted }`
- DELETE `/api/connections/:id` → 204

## Import / export complet

- GET `/api/export` → `Bundle`
- POST `/api/import` `Bundle` → `{ members, accounts, holdings, goals }` (compteurs). Idempotent : upsert par `external_id` (comptes) et par `name` (membres).

```ts
type Bundle = {
  version: 1;
  members: { name: string; color?: string; is_primary?: boolean }[];
  accounts: {
    external_id: string; name: string; institution?: string; kind: AccountKind; envelope?: Envelope;
    currency?: string; cash_balance?: number; annual_yield_pct?: number; opened_at?: string;
    owners: { member: string; share: number }[];      // member = nom
    holdings?: HoldingInput[]; property?: PropertyInput;
    loan?: LoanInput & { property_external_id?: string };
  }[];
  goals: (GoalInput sans account_ids/member_ids & { account_external_ids: string[]; members: string[] })[];
};
```

## Simulateurs

Projection financière, indépendance financière, simulateur de frais (contrats, ETF, autres) et simulateur de prêt
sont calculés **côté front** (pur TypeScript, instantané). Ils se pré-remplissent depuis `/api/overview`, `/api/goals`
et `weighted_expense_ratio`.
