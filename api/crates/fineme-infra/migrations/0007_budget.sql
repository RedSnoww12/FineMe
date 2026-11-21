-- Budget : choix manuels sur les transactions, règles de catégorisation par libellé et cibles mensuelles.
ALTER TABLE transactions ADD COLUMN manual_category TEXT;
-- NULL = détection automatique des virements internes ; true/false = marquage manuel.
ALTER TABLE transactions ADD COLUMN transfer_override BOOLEAN;
CREATE INDEX transactions_date_idx ON transactions(date);

CREATE TABLE budget_rules (
    id UUID PRIMARY KEY,
    pattern TEXT NOT NULL UNIQUE,   -- libellé normalisé
    category TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE budget_targets (
    category TEXT PRIMARY KEY,
    monthly_amount DOUBLE PRECISION NOT NULL CHECK (monthly_amount >= 0)
);
