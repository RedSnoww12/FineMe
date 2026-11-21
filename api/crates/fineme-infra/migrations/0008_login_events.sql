-- Journal des tentatives de connexion et de réinitialisation (90 jours conservés, purgés par l'application).
CREATE TABLE login_events (
    id BIGSERIAL PRIMARY KEY,
    at TIMESTAMPTZ NOT NULL,
    action TEXT NOT NULL CHECK (action IN ('login', 'reset')),
    success BOOLEAN NOT NULL,
    ip TEXT,
    user_agent TEXT
);

CREATE INDEX login_events_at_idx ON login_events (at DESC);
