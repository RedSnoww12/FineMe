-- Préférences d'import CSV : correspondance champ → nom de colonne, par compte et type de contenu.
-- `fields[i]` est alimenté par la colonne nommée `headers[i]`.
CREATE TABLE import_profiles (
    account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    template TEXT,
    fields TEXT[] NOT NULL,
    headers TEXT[] NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (account_id, kind)
);
