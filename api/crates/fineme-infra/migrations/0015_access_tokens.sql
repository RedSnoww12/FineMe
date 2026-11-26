-- Jetons d'accès personnels en lecture seule (serveur MCP) : seule l'empreinte SHA-256 du secret est conservée.
CREATE TABLE access_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name TEXT NOT NULL CHECK (char_length(name) BETWEEN 1 AND 60),
    scope TEXT NOT NULL CHECK (scope IN ('read')),
    secret_digest TEXT NOT NULL UNIQUE CHECK (secret_digest ~ '^[0-9a-f]{64}$'),
    hint TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ
);

CREATE INDEX access_tokens_user_idx ON access_tokens (user_id, created_at DESC);

-- Journal des appels de tools MCP (90 jours conservés, purgés par la tâche quotidienne). Le nom du jeton est
-- recopié : le journal reste lisible si le jeton disparaît.
CREATE TABLE mcp_calls (
    id BIGSERIAL PRIMARY KEY,
    at TIMESTAMPTZ NOT NULL,
    token_id UUID NOT NULL,
    token_name TEXT NOT NULL,
    tool TEXT NOT NULL,
    success BOOLEAN NOT NULL
);

CREATE INDEX mcp_calls_at_idx ON mcp_calls (at DESC);
