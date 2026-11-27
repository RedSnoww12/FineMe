//! Jetons d'accès personnels (empreinte du secret seulement) et journal des appels MCP.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fineme_application::ports::{AccessTokenRepository, McpCallJournal, RepoError, RepoResult};
use fineme_domain::access_token::{
    AccessToken, AccessTokenName, McpCall, SecretDigest, TokenScope,
};
use fineme_domain::ids::{AccessTokenId, UserId};
use sqlx::FromRow;
use uuid::Uuid;

use super::{PgStore, db};

const TOKEN_COLUMNS: &str = "SELECT id, user_id, name, scope, hint, created_at, last_used_at, revoked_at, \
     secret_digest FROM access_tokens";

#[derive(FromRow)]
struct AccessTokenRow {
    id: Uuid,
    user_id: Uuid,
    name: String,
    scope: String,
    hint: String,
    created_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    secret_digest: String,
}

impl AccessTokenRow {
    fn into_token(self) -> RepoResult<(AccessToken, SecretDigest)> {
        let corrupt =
            || RepoError::Backend(format!("donnée invalide dans access_tokens ({})", self.id));
        let name = AccessTokenName::parse(&self.name).map_err(|_| corrupt())?;
        let scope = TokenScope::parse(&self.scope).ok_or_else(corrupt)?;
        let token = AccessToken {
            id: AccessTokenId::from_uuid(self.id),
            user: UserId::from_uuid(self.user_id),
            name,
            scope,
            hint: self.hint,
            created_at: self.created_at,
            last_used_at: self.last_used_at,
            revoked_at: self.revoked_at,
        };
        let digest = SecretDigest::from_hex(self.secret_digest).ok_or_else(corrupt)?;
        Ok((token, digest))
    }
}

#[async_trait]
impl AccessTokenRepository for PgStore {
    async fn insert(&self, token: &AccessToken, digest: &SecretDigest) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO access_tokens (id, user_id, name, scope, secret_digest, hint, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(token.id.as_uuid())
        .bind(token.user.as_uuid())
        .bind(token.name.as_str())
        .bind(token.scope.as_str())
        .bind(digest.as_str())
        .bind(&token.hint)
        .bind(token.created_at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn list(&self, user: UserId) -> RepoResult<Vec<AccessToken>> {
        let rows: Vec<AccessTokenRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{TOKEN_COLUMNS} WHERE user_id = $1 ORDER BY created_at DESC, id"
        )))
        .bind(user.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter()
            .map(|row| row.into_token().map(|(token, _)| token))
            .collect()
    }

    async fn find(&self, id: AccessTokenId) -> RepoResult<Option<AccessToken>> {
        let row: Option<AccessTokenRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{TOKEN_COLUMNS} WHERE id = $1"
        )))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.map(|row| row.into_token().map(|(token, _)| token))
            .transpose()
    }

    async fn find_active_by_digest(
        &self,
        digest: &SecretDigest,
    ) -> RepoResult<Option<(AccessToken, SecretDigest)>> {
        let row: Option<AccessTokenRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{TOKEN_COLUMNS} WHERE secret_digest = $1 AND revoked_at IS NULL"
        )))
        .bind(digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.map(AccessTokenRow::into_token).transpose()
    }

    async fn revoke(&self, id: AccessTokenId, user: UserId, at: DateTime<Utc>) -> RepoResult<bool> {
        let result = sqlx::query(
            "UPDATE access_tokens SET revoked_at = $3 \
             WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL",
        )
        .bind(id.as_uuid())
        .bind(user.as_uuid())
        .bind(at)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(result.rows_affected() == 1)
    }

    async fn touch(&self, id: AccessTokenId, at: DateTime<Utc>) -> RepoResult<()> {
        sqlx::query("UPDATE access_tokens SET last_used_at = $2 WHERE id = $1")
            .bind(id.as_uuid())
            .bind(at)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }
}

#[derive(FromRow)]
struct McpCallRow {
    at: DateTime<Utc>,
    token_id: Uuid,
    token_name: String,
    tool: String,
    success: bool,
}

#[async_trait]
impl McpCallJournal for PgStore {
    async fn record(&self, call: &McpCall) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO mcp_calls (at, token_id, token_name, tool, success) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(call.at)
        .bind(call.token_id)
        .bind(&call.token_name)
        .bind(&call.tool)
        .bind(call.success)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn recent(&self, limit: i64) -> RepoResult<Vec<McpCall>> {
        let rows: Vec<McpCallRow> = sqlx::query_as(
            "SELECT at, token_id, token_name, tool, success FROM mcp_calls ORDER BY at DESC, id DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .into_iter()
            .map(|row| McpCall {
                at: row.at,
                token_id: row.token_id,
                token_name: row.token_name,
                tool: row.tool,
                success: row.success,
            })
            .collect())
    }

    async fn prune_before(&self, before: DateTime<Utc>) -> RepoResult<u64> {
        Ok(sqlx::query("DELETE FROM mcp_calls WHERE at < $1")
            .bind(before)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected())
    }
}
