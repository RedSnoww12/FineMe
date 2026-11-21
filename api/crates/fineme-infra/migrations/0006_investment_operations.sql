-- Journal d'opérations des comptes d'investissement : achats, ventes, dividendes, frais, versements, retraits.
-- Achat/vente : holding_id, quantity, unit_price et fees renseignés, amount nul (= quantity × unit_price).
-- Autres : amount renseigné ; holding_id facultatif (dividende, frais) ou nul (versement, retrait).
CREATE TABLE investment_operations (
    id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    -- Supprimer une position supprime son journal.
    holding_id UUID REFERENCES holdings(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    date DATE NOT NULL,
    quantity DOUBLE PRECISION,
    unit_price DOUBLE PRECISION,
    amount DOUBLE PRECISION,
    fees DOUBLE PRECISION NOT NULL DEFAULT 0,
    currency TEXT NOT NULL,
    note TEXT,
    -- Moment de la saisie : départage les opérations d'un même jour.
    recorded_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX investment_operations_account_idx ON investment_operations(account_id, date);
CREATE INDEX investment_operations_holding_idx ON investment_operations(holding_id);
