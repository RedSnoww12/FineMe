-- Détail des instantanés : valeur détenue de chaque compte, par jour et par vue (NULL = foyer).
-- Pas de clé étrangère vers les comptes : l'historique garde un compte supprimé depuis, sous son nom du jour.
-- `snapshots.detailed` distingue un instantané sans compte d'un instantané antérieur au détail.
ALTER TABLE snapshots ADD COLUMN detailed BOOLEAN NOT NULL DEFAULT false;

CREATE TABLE snapshot_accounts (
    date DATE NOT NULL,
    member_id UUID REFERENCES members(id) ON DELETE CASCADE,
    account_id UUID NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    gain DOUBLE PRECISION,
    UNIQUE NULLS NOT DISTINCT (date, member_id, account_id)
);
