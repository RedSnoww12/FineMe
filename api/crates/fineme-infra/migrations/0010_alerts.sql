-- Alertes et rappels : configuration des règles et alertes enregistrées (une par condition).

-- Une ligne par règle modifiée ; une règle absente prend sa configuration par défaut.
CREATE TABLE alert_rules (
    kind TEXT PRIMARY KEY,
    enabled BOOLEAN NOT NULL,
    threshold_pct DOUBLE PRECISION,
    ceiling DOUBLE PRECISION
);

CREATE TABLE alerts (
    id UUID PRIMARY KEY,
    -- Identifie la condition (ex. savings_ceiling:<compte>) : une même condition ne crée qu'une alerte.
    key TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    severity TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    read_at TIMESTAMPTZ,
    archived_at TIMESTAMPTZ,
    resolved_at TIMESTAMPTZ
);
CREATE INDEX alerts_created_at_idx ON alerts(created_at DESC);
