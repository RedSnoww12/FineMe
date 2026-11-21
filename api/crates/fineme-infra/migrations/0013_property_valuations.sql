-- Historique des estimations de valeur d'un bien immobilier.
-- La valeur courante d'un bien est celle de son estimation la plus récente à date ;
-- `properties.current_value` en reste une copie (valeur d'un bien sans estimation, lecture SQL directe).
CREATE TABLE property_valuations (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES properties(account_id) ON DELETE CASCADE,
    valued_on DATE NOT NULL,
    value DOUBLE PRECISION NOT NULL CHECK (value > 0),
    source TEXT NOT NULL,
    price_per_m2 DOUBLE PRECISION,
    note TEXT,
    recorded_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX property_valuations_account_idx ON property_valuations (account_id, valued_on);

-- Reprise : la valeur actuelle de chaque bien devient sa première estimation, saisie à la main et datée
-- du jour de création du compte (date UTC) — date la plus ancienne connue à laquelle cette valeur était
-- déclarée. Un bien de valeur nulle n'a pas d'estimation (une estimation est strictement positive).
INSERT INTO property_valuations (id, account_id, valued_on, value, source, recorded_at)
SELECT gen_random_uuid(), p.account_id, (a.created_at AT TIME ZONE 'UTC')::date, p.current_value, 'manual', a.created_at
FROM properties p
JOIN accounts a ON a.id = p.account_id
WHERE p.current_value > 0;
