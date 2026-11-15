CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE members (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    color TEXT NOT NULL,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE connections (
    id UUID PRIMARY KEY,
    provider TEXT NOT NULL,
    institution TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    last_sync_at TIMESTAMPTZ,
    error TEXT,
    external_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE accounts (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    institution TEXT,
    kind TEXT NOT NULL,
    envelope TEXT,
    currency TEXT NOT NULL DEFAULT 'EUR',
    cash_balance DOUBLE PRECISION NOT NULL DEFAULT 0,
    annual_yield_pct DOUBLE PRECISION,
    source TEXT NOT NULL DEFAULT 'manual',
    external_id TEXT UNIQUE,
    connection_id UUID REFERENCES connections(id) ON DELETE SET NULL,
    opened_at DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE account_owners (
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    member_id UUID NOT NULL REFERENCES members(id) ON DELETE RESTRICT,
    share DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (account_id, member_id)
);

CREATE TABLE holdings (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL DEFAULT 'security',
    name TEXT NOT NULL,
    isin TEXT,
    symbol TEXT,
    security_type TEXT,
    quantity DOUBLE PRECISION NOT NULL DEFAULT 0,
    buy_price DOUBLE PRECISION,
    current_price DOUBLE PRECISION NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'EUR',
    expense_ratio DOUBLE PRECISION,
    price_source TEXT NOT NULL DEFAULT 'manual',
    price_updated_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX holdings_account_idx ON holdings(account_id);

CREATE TABLE transactions (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    date DATE NOT NULL,
    label TEXT NOT NULL,
    amount DOUBLE PRECISION NOT NULL,
    category TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (account_id, date, label, amount)
);

CREATE TABLE properties (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    address TEXT,
    category TEXT NOT NULL DEFAULT 'primary',
    building_type TEXT NOT NULL DEFAULT 'apartment',
    surface_m2 DOUBLE PRECISION,
    purchase_price DOUBLE PRECISION NOT NULL DEFAULT 0,
    purchase_fees DOUBLE PRECISION NOT NULL DEFAULT 0,
    purchased_at DATE,
    current_value DOUBLE PRECISION NOT NULL DEFAULT 0,
    monthly_rent DOUBLE PRECISION NOT NULL DEFAULT 0,
    monthly_charges DOUBLE PRECISION NOT NULL DEFAULT 0,
    yearly_property_tax DOUBLE PRECISION NOT NULL DEFAULT 0
);

CREATE TABLE loans (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL UNIQUE REFERENCES accounts(id) ON DELETE CASCADE,
    property_account_id UUID REFERENCES accounts(id) ON DELETE SET NULL,
    principal DOUBLE PRECISION NOT NULL,
    interest_rate_pct DOUBLE PRECISION NOT NULL,
    insurance_rate_pct DOUBLE PRECISION NOT NULL DEFAULT 0,
    duration_months INTEGER NOT NULL,
    start_date DATE NOT NULL
);

-- Taux de change : unités de `currency` pour 1 EUR.
CREATE TABLE fx_rates (
    currency TEXT PRIMARY KEY,
    rate DOUBLE PRECISION NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Instantanés quotidiens ; member_id NULL = vue foyer.
CREATE TABLE snapshots (
    date DATE NOT NULL,
    member_id UUID REFERENCES members(id) ON DELETE CASCADE,
    gross_assets DOUBLE PRECISION NOT NULL,
    liabilities DOUBLE PRECISION NOT NULL,
    net_worth DOUBLE PRECISION NOT NULL,
    UNIQUE NULLS NOT DISTINCT (date, member_id)
);

CREATE TABLE goals (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    icon TEXT,
    kind TEXT NOT NULL,
    target_amount DOUBLE PRECISION,
    target_date DATE,
    monthly_contribution DOUBLE PRECISION,
    expected_return_pct DOUBLE PRECISION,
    monthly_expenses DOUBLE PRECISION,
    coverage_months DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE goal_accounts (
    goal_id UUID NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    PRIMARY KEY (goal_id, account_id)
);

CREATE TABLE goal_members (
    goal_id UUID NOT NULL REFERENCES goals(id) ON DELETE CASCADE,
    member_id UUID NOT NULL REFERENCES members(id) ON DELETE CASCADE,
    PRIMARY KEY (goal_id, member_id)
);
