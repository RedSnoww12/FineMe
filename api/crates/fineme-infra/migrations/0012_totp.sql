-- Second facteur (TOTP) : secret chiffré (AES-256-GCM, NULL = désactivé), secret en attente de confirmation
-- et dernier pas accepté (anti-rejeu).
ALTER TABLE users ADD COLUMN totp_secret TEXT;
ALTER TABLE users ADD COLUMN totp_pending_secret TEXT;
ALTER TABLE users ADD COLUMN totp_last_step BIGINT;

-- Codes de secours à usage unique, hachés en argon2id.
CREATE TABLE totp_recovery_codes (
    id BIGSERIAL PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash TEXT NOT NULL,
    used_at TIMESTAMPTZ
);

CREATE INDEX totp_recovery_codes_user_idx ON totp_recovery_codes (user_id);

-- Journal : seconde étape de connexion.
ALTER TABLE login_events DROP CONSTRAINT login_events_action_check;
ALTER TABLE login_events ADD CONSTRAINT login_events_action_check
    CHECK (action IN ('login', 'totp', 'reset'));
