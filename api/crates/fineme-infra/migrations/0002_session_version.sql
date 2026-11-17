-- Version de session : incrémentée à chaque changement de mot de passe pour invalider les jetons émis avant.
-- Les jetons existants (sans version) valent 0 et restent donc valables jusqu'au premier changement.
ALTER TABLE users ADD COLUMN session_version BIGINT NOT NULL DEFAULT 0;
