# FineMe — contrat d'API (source de vérité front ↔ back)

Application personnelle de suivi de patrimoine (un seul login, plusieurs membres du foyer).

- `api/` : Rust (Axum + sqlx + Postgres), hébergé sur Render, base Postgres Supabase.
- `web/` : React + Vite + TypeScript, hébergé sur Vercel.
- Toutes les routes sont préfixées par `/api`. JSON partout, `snake_case`.
- Auth : `Authorization: Bearer <JWT>` sur toutes les routes sauf `/api/health`, `/api/auth/status`, `/api/auth/setup`, `/api/auth/login`.
- Montants : nombres JSON (f64). Devise de référence : EUR. Dates : `YYYY-MM-DD`. Identifiants : UUID.
- Parts de détention : fraction `0..1` (0.5 = 50 %).
- Erreurs : statut HTTP + `{ "error": "message lisible en français" }`.
- CORS : origines autorisées via `CORS_ORIGINS` (liste séparée par des virgules).

## Variables d'environnement (api)

`DATABASE_URL`, `JWT_SECRET`, `CORS_ORIGINS`, `PORT` (défaut 8080), `SETUP_TOKEN` (optionnel en local, à définir en production : code exigé pour la première configuration, afin que personne d'autre ne puisse créer le compte).
Les migrations sqlx s'exécutent au démarrage.

## Auth

| Méthode | Route | Corps | Réponse |
|---|---|---|---|
| GET | `/api/health` | – | `{ "status": "ok" }` |
| GET | `/api/auth/status` | – | `{ "setup_required": bool, "setup_token_required": bool }` (`setup_required` true tant qu'aucun utilisateur n'existe ; `setup_token_required` true si `SETUP_TOKEN` est défini côté serveur) |
| POST | `/api/auth/setup` | `{ email, password, name, setup_token? }` | `{ token, user }` — refusé (409) si un utilisateur existe déjà ; refusé (403) si `SETUP_TOKEN` est défini et que `setup_token` est absent ou différent (comparaison en temps constant). Crée aussi le membre principal `name`. |
| POST | `/api/auth/login` | `{ email, password }` | `{ token, user }` |
| GET | `/api/auth/me` | – | `user` = `{ id, email, name }` |

Mot de passe ≥ 10 caractères, hashé en argon2id. JWT valable 30 jours.

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
  taux de change via frankfurter.app. Un échec ne casse jamais la requête ; le prix manuel reste.
- GET `/api/fx` → `{ base: "EUR", rates: { "USD": 1.10, ... }, updated_at }` (rates = unités de devise pour 1 EUR)

## Transactions (import CSV / agrégateur)

`Transaction = { id, account_id, date, label, amount, category: string | null }` (amount signé, devise du compte)

- GET `/api/accounts/:id/transactions?limit=&offset=` → `{ items: Transaction[], total: number }`
- POST `/api/accounts/:id/transactions/bulk` `{ transactions: [{date,label,amount,category?}], new_cash_balance?: number }` → `{ inserted, skipped }`
  (dédoublonnage sur date+label+amount)
- DELETE `/api/transactions/:id` → 204

Le parsing du CSV et le mapping des colonnes se font côté front.

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
GET `/api/history?range=1m|3m|1y|all` → `{ points: [{ date, gross_assets, liabilities, net_worth }] }`

Un instantané du jour (foyer + chaque membre) est enregistré/mis à jour à chaque appel de `/api/overview`.
`/api/history` respecte `member_id`.

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
