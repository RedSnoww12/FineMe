-- Serveur d'autorisation OAuth (connecteur claude.ai). Seules les empreintes SHA-256 des secrets sont conservées.

-- Version du seul mot de passe : session_version avance aussi avec le second facteur, qui ne doit pas couper les
-- autorisations.
ALTER TABLE users ADD COLUMN password_version BIGINT NOT NULL DEFAULT 0;

-- Demandes en attente du consentement (route publique : bornées par l'adaptateur).
CREATE TABLE oauth_requests (
    digest TEXT PRIMARY KEY CHECK (digest ~ '^[0-9a-f]{64}$'),
    seq BIGSERIAL NOT NULL,
    client_id TEXT NOT NULL,
    client_name TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    state TEXT,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('read')),
    expires_at TIMESTAMPTZ NOT NULL
);

-- Codes d'autorisation. Un code utilisé reste jusqu'à son expiration pour reconnaître un rejeu.
CREATE TABLE oauth_codes (
    digest TEXT PRIMARY KEY CHECK (digest ~ '^[0-9a-f]{64}$'),
    grant_id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    password_version BIGINT NOT NULL,
    client_id TEXT NOT NULL,
    client_name TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    state TEXT,
    resource TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('read')),
    expires_at TIMESTAMPTZ NOT NULL,
    used BOOLEAN NOT NULL DEFAULT FALSE,
    replayed BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE oauth_grants (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    client_id TEXT NOT NULL,
    client_name TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL CHECK (scope IN ('read')),
    resource TEXT NOT NULL,
    password_version BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX oauth_grants_user_idx ON oauth_grants (user_id, created_at DESC);
-- Une seule autorisation active par client.
CREATE UNIQUE INDEX oauth_grants_active_client_idx ON oauth_grants (user_id, client_id) WHERE revoked_at IS NULL;

-- Jetons d'accès et de renouvellement. Un jeton de renouvellement tourné reste pour reconnaître un rejeu.
CREATE TABLE oauth_tokens (
    digest TEXT PRIMARY KEY CHECK (digest ~ '^[0-9a-f]{64}$'),
    grant_id UUID NOT NULL REFERENCES oauth_grants (id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN ('access', 'refresh')),
    expires_at TIMESTAMPTZ NOT NULL,
    used BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX oauth_tokens_grant_idx ON oauth_tokens (grant_id);

-- Le consentement est journalisé comme une connexion.
ALTER TABLE login_events DROP CONSTRAINT login_events_action_check;
ALTER TABLE login_events ADD CONSTRAINT login_events_action_check
    CHECK (action IN ('login', 'totp', 'reset', 'oauth'));
