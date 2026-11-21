//! Alertes et configuration de leurs règles.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use fineme_application::ports::{AlertRepository, RepoError, RepoResult};
use fineme_domain::alert::{Alert, AlertKind, AlertRule, AlertRuleKind, AlertSeverity};
use fineme_domain::ids::AlertId;
use sqlx::FromRow;
use uuid::Uuid;

use super::rows::corrupt;
use super::{PgStore, db};

const ALERT_COLUMNS: &str = "SELECT id, key, kind, severity, title, message, created_at, read_at, \
     archived_at, resolved_at FROM alerts";

#[derive(FromRow)]
struct AlertRow {
    id: Uuid,
    key: String,
    kind: String,
    severity: String,
    title: String,
    message: String,
    created_at: DateTime<Utc>,
    read_at: Option<DateTime<Utc>>,
    archived_at: Option<DateTime<Utc>>,
    resolved_at: Option<DateTime<Utc>>,
}

impl TryFrom<AlertRow> for Alert {
    type Error = RepoError;

    fn try_from(row: AlertRow) -> RepoResult<Self> {
        Ok(Alert {
            id: AlertId::from_uuid(row.id),
            kind: AlertKind::parse(&row.kind).map_err(corrupt("alerts", row.id))?,
            severity: AlertSeverity::parse(&row.severity).map_err(corrupt("alerts", row.id))?,
            key: row.key,
            title: row.title,
            message: row.message,
            created_at: row.created_at,
            read_at: row.read_at,
            archived_at: row.archived_at,
            resolved_at: row.resolved_at,
        })
    }
}

#[derive(FromRow)]
struct AlertRuleRow {
    kind: String,
    enabled: bool,
    threshold_pct: Option<f64>,
    ceiling: Option<f64>,
}

impl TryFrom<AlertRuleRow> for AlertRule {
    type Error = RepoError;

    fn try_from(row: AlertRuleRow) -> RepoResult<Self> {
        AlertRuleKind::parse(&row.kind)
            .and_then(|kind| AlertRule::new(kind, row.enabled, row.threshold_pct, row.ceiling))
            .map_err(|error| {
                RepoError::Backend(format!(
                    "donnée invalide dans alert_rules ({}) : {error}",
                    row.kind
                ))
            })
    }
}

#[async_trait]
impl AlertRepository for PgStore {
    async fn alerts(&self) -> RepoResult<Vec<Alert>> {
        sqlx::query_as::<_, AlertRow>(sqlx::AssertSqlSafe(format!(
            "{ALERT_COLUMNS} ORDER BY created_at DESC, key"
        )))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(Alert::try_from)
        .collect()
    }

    async fn find_alert(&self, id: AlertId) -> RepoResult<Option<Alert>> {
        sqlx::query_as::<_, AlertRow>(sqlx::AssertSqlSafe(format!(
            "{ALERT_COLUMNS} WHERE id = $1"
        )))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .map(Alert::try_from)
        .transpose()
    }

    async fn save_alerts(&self, alerts: &[Alert]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for a in alerts {
            sqlx::query(
                "INSERT INTO alerts (id, key, kind, severity, title, message, created_at, read_at, \
                 archived_at, resolved_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
                 ON CONFLICT (id) DO UPDATE SET key = EXCLUDED.key, kind = EXCLUDED.kind, \
                 severity = EXCLUDED.severity, title = EXCLUDED.title, message = EXCLUDED.message, \
                 created_at = EXCLUDED.created_at, read_at = EXCLUDED.read_at, \
                 archived_at = EXCLUDED.archived_at, resolved_at = EXCLUDED.resolved_at",
            )
            .bind(a.id.as_uuid())
            .bind(&a.key)
            .bind(a.kind.as_str())
            .bind(a.severity.as_str())
            .bind(&a.title)
            .bind(&a.message)
            .bind(a.created_at)
            .bind(a.read_at)
            .bind(a.archived_at)
            .bind(a.resolved_at)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn rules(&self) -> RepoResult<Vec<AlertRule>> {
        sqlx::query_as::<_, AlertRuleRow>(
            "SELECT kind, enabled, threshold_pct, ceiling FROM alert_rules ORDER BY kind",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(AlertRule::try_from)
        .collect()
    }

    async fn save_rule(&self, rule: &AlertRule) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO alert_rules (kind, enabled, threshold_pct, ceiling) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (kind) DO UPDATE SET enabled = EXCLUDED.enabled, \
             threshold_pct = EXCLUDED.threshold_pct, ceiling = EXCLUDED.ceiling",
        )
        .bind(rule.kind().as_str())
        .bind(rule.enabled())
        .bind(rule.threshold_pct())
        .bind(rule.ceiling())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }
}
