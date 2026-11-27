//! Serveur d'autorisation OAuth : demandes, codes, autorisations et jetons (empreintes des secrets seulement).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fineme_application::ports::{
    AccessRecord, AuthorizationCode, CodeUse, IssuedPair, MAX_PENDING_REQUESTS, OAuthRepository,
    PendingRequest, RefreshRecord, RepoError, RepoResult,
};
use fineme_domain::access_token::{SecretDigest, TokenScope};
use fineme_domain::ids::{OAuthGrantId, UserId};
use fineme_domain::oauth::{
    AuthorizationRequest, ClientId, ClientName, ClientState, CodeChallenge, OAuthGrant,
    ProtectedResource, RedirectUri,
};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{PgStore, db};

fn corrupt(table: &str) -> RepoError {
    RepoError::Backend(format!("donnée invalide dans {table}"))
}

#[derive(FromRow)]
struct RequestColumns {
    client_id: String,
    client_name: String,
    redirect_uri: String,
    code_challenge: String,
    state: Option<String>,
    resource: String,
    scope: String,
}

impl RequestColumns {
    fn into_request(self, table: &str) -> RepoResult<AuthorizationRequest> {
        let invalid = || corrupt(table);
        let state = match self.state {
            Some(state) => Some(ClientState::parse(&state).ok_or_else(invalid)?),
            None => None,
        };
        Ok(AuthorizationRequest {
            client_id: ClientId::parse(&self.client_id).ok_or_else(invalid)?,
            client_name: ClientName::parse(&self.client_name).ok_or_else(invalid)?,
            redirect_uri: RedirectUri::parse(&self.redirect_uri).ok_or_else(invalid)?,
            code_challenge: CodeChallenge::parse(&self.code_challenge).ok_or_else(invalid)?,
            state,
            resource: ProtectedResource::parse(&self.resource).ok_or_else(invalid)?,
            scope: TokenScope::parse(&self.scope).ok_or_else(invalid)?,
        })
    }
}

#[derive(FromRow)]
struct PendingRow {
    #[sqlx(flatten)]
    request: RequestColumns,
    expires_at: DateTime<Utc>,
}

#[derive(FromRow)]
struct CodeRow {
    #[sqlx(flatten)]
    request: RequestColumns,
    grant_id: Uuid,
    user_id: Uuid,
    password_version: i64,
    expires_at: DateTime<Utc>,
}

impl CodeRow {
    fn into_code(self) -> RepoResult<AuthorizationCode> {
        Ok(AuthorizationCode {
            request: self.request.into_request("oauth_codes")?,
            grant_id: OAuthGrantId::from_uuid(self.grant_id),
            user: UserId::from_uuid(self.user_id),
            password_version: self.password_version,
            expires_at: self.expires_at,
        })
    }
}

#[derive(FromRow)]
struct GrantRow {
    id: Uuid,
    user_id: Uuid,
    client_id: String,
    client_name: String,
    redirect_uri: String,
    scope: String,
    resource: String,
    password_version: i64,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

impl GrantRow {
    fn into_grant(self) -> RepoResult<OAuthGrant> {
        let invalid = || corrupt("oauth_grants");
        Ok(OAuthGrant {
            id: OAuthGrantId::from_uuid(self.id),
            user: UserId::from_uuid(self.user_id),
            client_id: ClientId::parse(&self.client_id).ok_or_else(invalid)?,
            client_name: ClientName::parse(&self.client_name).ok_or_else(invalid)?,
            redirect_uri: RedirectUri::parse(&self.redirect_uri).ok_or_else(invalid)?,
            scope: TokenScope::parse(&self.scope).ok_or_else(invalid)?,
            resource: ProtectedResource::parse(&self.resource).ok_or_else(invalid)?,
            password_version: self.password_version,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
            revoked_at: self.revoked_at,
        })
    }
}

#[derive(FromRow)]
struct TokenRow {
    #[sqlx(flatten)]
    grant: GrantRow,
    digest: String,
    expires_at: DateTime<Utc>,
    used: bool,
}

const GRANT_COLUMNS: &str = "g.id, g.user_id, g.client_id, g.client_name, g.redirect_uri, g.scope, g.resource, \
     g.password_version, g.created_at, g.last_used_at, g.revoked_at";

const REQUEST_COLUMNS: &str =
    "client_id, client_name, redirect_uri, code_challenge, state, resource, scope";

impl PgStore {
    async fn find_token(&self, digest: &SecretDigest, kind: &str) -> RepoResult<Option<TokenRow>> {
        sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {GRANT_COLUMNS}, t.digest, t.expires_at, t.used FROM oauth_tokens t \
             JOIN oauth_grants g ON g.id = t.grant_id WHERE t.digest = $1 AND t.kind = $2"
        )))
        .bind(digest.as_str())
        .bind(kind)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }
}

async fn insert_pair(
    tx: &mut PgConnection,
    grant: OAuthGrantId,
    pair: &IssuedPair,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO oauth_tokens (digest, grant_id, kind, expires_at) \
         VALUES ($1, $3, 'access', $2), ($4, $3, 'refresh', $5)",
    )
    .bind(pair.access.as_str())
    .bind(pair.access_expires_at)
    .bind(grant.as_uuid())
    .bind(pair.refresh.as_str())
    .bind(pair.refresh_expires_at)
    .execute(tx)
    .await
    .map_err(db)?;
    Ok(())
}

#[async_trait]
impl OAuthRepository for PgStore {
    async fn save_request(
        &self,
        digest: &SecretDigest,
        pending: &PendingRequest,
        now: DateTime<Utc>,
    ) -> RepoResult<()> {
        let request = &pending.request;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO oauth_requests (digest, client_id, client_name, redirect_uri, code_challenge, state, \
             resource, scope, expires_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(digest.as_str())
        .bind(request.client_id.as_str())
        .bind(request.client_name.as_str())
        .bind(request.redirect_uri.as_str())
        .bind(request.code_challenge.as_str())
        .bind(request.state.as_ref().map(ClientState::as_str))
        .bind(request.resource.as_str())
        .bind(request.scope.as_str())
        .bind(pending.expires_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "DELETE FROM oauth_requests WHERE expires_at <= $1 OR digest IN \
             (SELECT digest FROM oauth_requests ORDER BY seq DESC OFFSET $2)",
        )
        .bind(now)
        .bind(i64::try_from(MAX_PENDING_REQUESTS).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }

    async fn find_request(&self, digest: &SecretDigest) -> RepoResult<Option<PendingRequest>> {
        let row: Option<PendingRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {REQUEST_COLUMNS}, expires_at FROM oauth_requests WHERE digest = $1"
        )))
        .bind(digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.map(|row| {
            Ok(PendingRequest {
                request: row.request.into_request("oauth_requests")?,
                expires_at: row.expires_at,
            })
        })
        .transpose()
    }

    async fn take_request(&self, digest: &SecretDigest) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM oauth_requests WHERE digest = $1")
            .bind(digest.as_str())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() == 1)
    }

    async fn save_code(&self, digest: &SecretDigest, code: &AuthorizationCode) -> RepoResult<()> {
        let request = &code.request;
        sqlx::query(
            "INSERT INTO oauth_codes (digest, grant_id, user_id, password_version, client_id, client_name, \
             redirect_uri, code_challenge, state, resource, scope, expires_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(digest.as_str())
        .bind(code.grant_id.as_uuid())
        .bind(code.user.as_uuid())
        .bind(code.password_version)
        .bind(request.client_id.as_str())
        .bind(request.client_name.as_str())
        .bind(request.redirect_uri.as_str())
        .bind(request.code_challenge.as_str())
        .bind(request.state.as_ref().map(ClientState::as_str))
        .bind(request.resource.as_str())
        .bind(request.scope.as_str())
        .bind(code.expires_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn consume_code(
        &self,
        digest: &SecretDigest,
        _now: DateTime<Utc>,
    ) -> RepoResult<Option<CodeUse>> {
        // Écriture conditionnelle : une seule présentation obtient le code.
        let first: Option<CodeRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE oauth_codes SET used = TRUE WHERE digest = $1 AND NOT used \
             RETURNING {REQUEST_COLUMNS}, grant_id, user_id, password_version, expires_at"
        )))
        .bind(digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        if let Some(row) = first {
            return Ok(Some(CodeUse::First(Box::new(row.into_code()?))));
        }
        let replayed: Option<CodeRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "UPDATE oauth_codes SET replayed = TRUE WHERE digest = $1 \
             RETURNING {REQUEST_COLUMNS}, grant_id, user_id, password_version, expires_at"
        )))
        .bind(digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        replayed
            .map(|row| Ok(CodeUse::Replayed(Box::new(row.into_code()?))))
            .transpose()
    }

    async fn create_grant(&self, grant: &OAuthGrant, pair: &IssuedPair) -> RepoResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Verrou sur le code : un rejeu concurrent attend la création (puis la révoque) ou l'empêche.
        let replayed: Option<bool> =
            sqlx::query_scalar("SELECT replayed FROM oauth_codes WHERE grant_id = $1 FOR UPDATE")
                .bind(grant.id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        if replayed == Some(true) {
            return Ok(false);
        }
        sqlx::query(
            "UPDATE oauth_grants SET revoked_at = $3 \
             WHERE user_id = $1 AND client_id = $2 AND revoked_at IS NULL",
        )
        .bind(grant.user.as_uuid())
        .bind(grant.client_id.as_str())
        .bind(grant.created_at)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        let inserted = sqlx::query(
            "INSERT INTO oauth_grants (id, user_id, client_id, client_name, redirect_uri, scope, resource, \
             password_version, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(grant.id.as_uuid())
        .bind(grant.user.as_uuid())
        .bind(grant.client_id.as_str())
        .bind(grant.client_name.as_str())
        .bind(grant.redirect_uri.as_str())
        .bind(grant.scope.as_str())
        .bind(grant.resource.as_str())
        .bind(grant.password_version)
        .bind(grant.created_at)
        .execute(&mut *tx)
        .await
        .map_err(db);
        match inserted {
            // Deux échanges simultanés pour le même client : l'index unique n'en laisse passer qu'un.
            Err(RepoError::Conflict) => return Ok(false),
            other => other?,
        };
        insert_pair(&mut tx, grant.id, pair).await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    async fn grants(&self, user: UserId) -> RepoResult<Vec<OAuthGrant>> {
        let rows: Vec<GrantRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT {GRANT_COLUMNS} FROM oauth_grants g WHERE g.user_id = $1 ORDER BY g.created_at DESC, g.id"
        )))
        .bind(user.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter().map(GrantRow::into_grant).collect()
    }

    async fn revoke_grant(&self, id: OAuthGrantId, at: DateTime<Utc>) -> RepoResult<bool> {
        let result = sqlx::query(
            "UPDATE oauth_grants SET revoked_at = $2 WHERE id = $1 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(result.rows_affected() == 1)
    }

    async fn touch_grant(&self, id: OAuthGrantId, at: DateTime<Utc>) -> RepoResult<()> {
        sqlx::query("UPDATE oauth_grants SET last_used_at = $2 WHERE id = $1")
            .bind(id.as_uuid())
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn find_access(&self, digest: &SecretDigest) -> RepoResult<Option<AccessRecord>> {
        self.find_token(digest, "access")
            .await?
            .map(|row| {
                Ok(AccessRecord {
                    digest: SecretDigest::from_hex(row.digest)
                        .ok_or_else(|| corrupt("oauth_tokens"))?,
                    expires_at: row.expires_at,
                    grant: row.grant.into_grant()?,
                })
            })
            .transpose()
    }

    async fn find_refresh(&self, digest: &SecretDigest) -> RepoResult<Option<RefreshRecord>> {
        self.find_token(digest, "refresh")
            .await?
            .map(|row| {
                Ok(RefreshRecord {
                    expires_at: row.expires_at,
                    used: row.used,
                    grant: row.grant.into_grant()?,
                })
            })
            .transpose()
    }

    async fn rotate(
        &self,
        old_refresh: &SecretDigest,
        grant: OAuthGrantId,
        pair: &IssuedPair,
    ) -> RepoResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let used = sqlx::query(
            "UPDATE oauth_tokens SET used = TRUE \
             WHERE digest = $1 AND grant_id = $2 AND kind = 'refresh' AND NOT used",
        )
        .bind(old_refresh.as_str())
        .bind(grant.as_uuid())
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if used.rows_affected() != 1 {
            return Ok(false);
        }
        insert_pair(&mut tx, grant, pair).await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    async fn prune_expired(&self, now: DateTime<Utc>) -> RepoResult<u64> {
        let mut removed = 0;
        for statement in [
            "DELETE FROM oauth_requests WHERE expires_at <= $1",
            "DELETE FROM oauth_codes WHERE expires_at <= $1",
            "DELETE FROM oauth_tokens WHERE expires_at <= $1",
        ] {
            removed += sqlx::query(statement)
                .bind(now)
                .execute(&self.pool)
                .await
                .map_err(db)?
                .rows_affected();
        }
        Ok(removed)
    }
}
