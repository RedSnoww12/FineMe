//! Alertes et rappels : évaluation des règles (tâche quotidienne ou à la demande), consultation,
//! état lu / archivé et configuration des règles.

use std::sync::Arc;

use chrono::Duration;
use fineme_domain::account::AccountKind;
use fineme_domain::alert::{
    Alert, AlertInputs, AlertRule, AlertRuleKind, AlertRules, GoalState, NetWorthPoint,
    SavingsBalance, evaluate, reconcile,
};
use fineme_domain::ids::AlertId;
use fineme_domain::portfolio::Portfolio;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{
    AlertRepository, Clock, ConnectionRepository, GoalRepository, SnapshotRepository,
};
use crate::reader::PortfolioReader;

/// Profondeur d'historique consultée pour comparer les deux derniers instantanés.
const NET_WORTH_LOOKBACK_DAYS: i64 = 31;
const ALERT_NOT_FOUND: &str = "Alerte introuvable";

/// Compteurs d'une évaluation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvaluationReport {
    pub created: usize,
    pub reopened: usize,
    pub resolved: usize,
}

/// Modification de l'état d'une alerte (champ absent = inchangé).
#[derive(Debug, Clone, Copy, Default)]
pub struct AlertPatch {
    pub read: Option<bool>,
    pub archived: Option<bool>,
}

/// Modification d'une règle (champ absent = inchangé).
#[derive(Debug, Clone, Copy, Default)]
pub struct RulePatch {
    pub enabled: Option<bool>,
    pub threshold_pct: Option<f64>,
    pub ceiling: Option<f64>,
}

pub struct AlertService {
    pub(crate) reader: PortfolioReader,
    pub(crate) goals: Arc<dyn GoalRepository>,
    pub(crate) snapshots: Arc<dyn SnapshotRepository>,
    pub(crate) connections: Arc<dyn ConnectionRepository>,
    pub(crate) alerts: Arc<dyn AlertRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Les livrets (comptes d'épargne) et leur valeur en EUR.
fn savings_balances(portfolio: &Portfolio) -> Vec<SavingsBalance> {
    portfolio
        .accounts
        .iter()
        .filter(|account| account.kind == AccountKind::Savings)
        .map(|account| SavingsBalance {
            account_id: account.id,
            name: account.name.clone(),
            value: portfolio.valuation(account).value,
        })
        .collect()
}

fn stale_prices(portfolio: &Portfolio) -> Vec<String> {
    let mut names: Vec<String> = portfolio
        .holdings
        .values()
        .flatten()
        .filter(|holding| holding.price_is_stale(portfolio.today))
        .map(|holding| holding.name.clone())
        .collect();
    names.sort();
    names
}

impl AlertService {
    async fn inputs(&self) -> AppResult<AlertInputs> {
        let portfolio = self.reader.load().await?;
        // Statut calculé exactement comme pour l'écran des objectifs.
        let goals = self
            .goals
            .list()
            .await?
            .into_iter()
            .map(|goal| {
                let current = portfolio.household_value_of(&goal.account_ids);
                GoalState {
                    status: goal.progress(current, portfolio.today).status,
                    id: goal.id,
                    name: goal.name,
                }
            })
            .collect();
        let since = portfolio.today - Duration::days(NET_WORTH_LOOKBACK_DAYS);
        let points = self.snapshots.since(None, since).await?;
        let net_worth = match points.as_slice() {
            [.., previous, latest] => Some((
                NetWorthPoint {
                    date: previous.date,
                    net_worth: previous.totals.net_worth,
                },
                NetWorthPoint {
                    date: latest.date,
                    net_worth: latest.totals.net_worth,
                },
            )),
            _ => None,
        };
        Ok(AlertInputs {
            goals,
            savings: savings_balances(&portfolio),
            net_worth,
            connections: self.connections.list().await?,
            stale_prices: stale_prices(&portfolio),
        })
    }

    async fn rule_set(&self) -> AppResult<AlertRules> {
        Ok(AlertRules::with_defaults(self.alerts.rules().await?))
    }

    /// Évalue les règles activées et enregistre les alertes nouvelles, rouvertes ou résolues.
    pub async fn evaluate(&self) -> AppResult<EvaluationReport> {
        let rules = self.rule_set().await?;
        let candidates = evaluate(&rules, &self.inputs().await?);
        let existing = self.alerts.alerts().await?;
        let result = reconcile(&existing, candidates, self.clock.now(), AlertId::generate);
        if !result.changed.is_empty() {
            self.alerts.save_alerts(&result.changed).await?;
        }
        Ok(EvaluationReport {
            created: result.created,
            reopened: result.reopened,
            resolved: result.resolved,
        })
    }

    /// Alertes non archivées (ou archivées si `archived`), de la plus récente à la plus ancienne.
    pub async fn list(&self, archived: bool) -> AppResult<Vec<Alert>> {
        let mut alerts: Vec<Alert> = self
            .alerts
            .alerts()
            .await?
            .into_iter()
            .filter(|alert| alert.archived_at.is_some() == archived)
            .collect();
        alerts.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.key.cmp(&b.key)));
        Ok(alerts)
    }

    pub async fn update(&self, id: Uuid, patch: AlertPatch) -> AppResult<Alert> {
        let mut alert = self
            .alerts
            .find_alert(AlertId::from_uuid(id))
            .await?
            .ok_or(AppError::NotFound(ALERT_NOT_FOUND))?;
        let now = self.clock.now();
        if let Some(read) = patch.read {
            alert.set_read(read, now);
        }
        if let Some(archived) = patch.archived {
            alert.set_archived(archived, now);
        }
        self.alerts
            .save_alerts(std::slice::from_ref(&alert))
            .await?;
        Ok(alert)
    }

    /// Marque lues toutes les alertes non archivées ; renvoie le nombre d'alertes modifiées.
    pub async fn read_all(&self) -> AppResult<usize> {
        let now = self.clock.now();
        let unread: Vec<Alert> = self
            .alerts
            .alerts()
            .await?
            .into_iter()
            .filter(|alert| alert.read_at.is_none() && alert.archived_at.is_none())
            .map(|mut alert| {
                alert.set_read(true, now);
                alert
            })
            .collect();
        if !unread.is_empty() {
            self.alerts.save_alerts(&unread).await?;
        }
        Ok(unread.len())
    }

    pub async fn rules(&self) -> AppResult<Vec<AlertRule>> {
        Ok(self.rule_set().await?.all().to_vec())
    }

    pub async fn update_rule(&self, kind: &str, patch: RulePatch) -> AppResult<AlertRule> {
        let kind = AlertRuleKind::parse(kind)
            .map_err(|_| AppError::NotFound("Règle d'alerte introuvable"))?;
        let rule = self.rule_set().await?.get(kind).updated(
            patch.enabled,
            patch.threshold_pct,
            patch.ceiling,
        )?;
        self.alerts.save_rule(&rule).await?;
        Ok(rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{AccountInput, GoalInput, OwnerInput};
    use crate::ports::SnapshotPoint;
    use crate::testing::{TestApp, holding_input};
    use fineme_domain::alert::AlertKind;
    use fineme_domain::connection::{Connection, ConnectionStatus};
    use fineme_domain::ids::ConnectionId;
    use fineme_domain::portfolio::Totals;

    async fn livret(t: &TestApp, cash: f64) {
        t.create(AccountInput {
            name: Some("Livret A".into()),
            kind: Some("savings".into()),
            cash_balance: Some(cash),
            owners: Some(vec![OwnerInput {
                member_id: t.alice.as_uuid(),
                share: 1.0,
            }]),
            ..AccountInput::default()
        })
        .await;
    }

    fn kinds(alerts: &[Alert]) -> Vec<AlertKind> {
        let mut kinds: Vec<AlertKind> = alerts.iter().map(|alert| alert.kind).collect();
        kinds.sort_by_key(|kind| kind.as_str());
        kinds
    }

    #[tokio::test]
    async fn evaluation_is_deduplicated_from_one_day_to_the_next() {
        let t = TestApp::household().await;
        livret(&t, 22_000.0).await;
        let first = t.app.alerts.evaluate().await.unwrap();
        assert_eq!(first.created, 1);
        let alerts = t.app.alerts.list(false).await.unwrap();
        assert_eq!(kinds(&alerts), [AlertKind::SavingsCeiling]);

        t.clock.advance_days(1);
        let second = t.app.alerts.evaluate().await.unwrap();
        assert_eq!(second, EvaluationReport::default());
        assert_eq!(t.app.alerts.list(false).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn goals_status_is_reused_for_reached_and_overdue_goals() {
        let t = TestApp::household().await;
        let account = t.checking(&[(t.alice, 1.0)], 1_000.0).await;
        for (name, target, target_date) in [
            ("Atteint", 500.0, None),
            (
                "En retard",
                5_000.0,
                Some(t.clock.today() - Duration::days(1)),
            ),
            ("En cours", 5_000.0, None),
        ] {
            let input = GoalInput {
                name: Some(name.into()),
                kind: Some("project".into()),
                target_amount: Some(Some(target)),
                target_date: Some(target_date),
                account_ids: Some(vec![account.as_uuid()]),
                ..GoalInput::default()
            };
            t.app.goals.create(&input).await.unwrap();
        }
        t.app.alerts.evaluate().await.unwrap();
        let alerts = t.app.alerts.list(false).await.unwrap();
        assert_eq!(
            kinds(&alerts),
            [AlertKind::GoalOverdue, AlertKind::GoalReached]
        );
    }

    #[tokio::test]
    async fn strong_change_between_the_last_two_snapshots_is_flagged() {
        let t = TestApp::household().await;
        let account = t.checking(&[(t.alice, 1.0)], 10_000.0).await;
        t.app.overview.record_snapshots().await.unwrap();
        t.clock.advance_days(1);
        let input = AccountInput {
            cash_balance: Some(15_000.0),
            ..AccountInput::default()
        };
        t.app
            .accounts
            .update(account.as_uuid(), &input)
            .await
            .unwrap();
        t.app.overview.record_snapshots().await.unwrap();
        t.app.alerts.evaluate().await.unwrap();
        let alerts = t.app.alerts.list(false).await.unwrap();
        assert_eq!(kinds(&alerts), [AlertKind::NetWorthChange]);
        assert!(alerts[0].title.contains("+50.0 %"), "{}", alerts[0].title);
    }

    #[tokio::test]
    async fn a_single_snapshot_or_an_old_one_does_not_compare() {
        let t = TestApp::household().await;
        t.checking(&[(t.alice, 1.0)], 10_000.0).await;
        let old = Totals {
            gross_assets: 1.0,
            liabilities: 0.0,
            net_worth: 1.0,
        };
        let long_ago = t.clock.today() - Duration::days(NET_WORTH_LOOKBACK_DAYS + 1);
        t.store.lock().snapshots.push((
            None,
            SnapshotPoint {
                date: long_ago,
                totals: old,
                accounts: None,
                corrected: false,
            },
        ));
        t.app.overview.record_snapshots().await.unwrap();
        assert_eq!(t.app.alerts.evaluate().await.unwrap().created, 0);
    }

    #[tokio::test]
    async fn failing_connections_and_stale_prices_are_flagged_then_resolved() {
        let t = TestApp::household().await;
        let connection = Connection {
            id: ConnectionId::generate(),
            provider: "demo".into(),
            institution: None,
            status: ConnectionStatus::Expired,
            last_sync_at: None,
            error: None,
            external_id: None,
        };
        t.store.lock().connections.push(connection.clone());
        let pea = t.investment(t.alice, 0.0).await;
        let input = holding_input("ETF Démo", 1.0, None, 10.0);
        let holding = t.app.holdings.create(pea.as_uuid(), &input).await.unwrap();
        // Cours automatique jamais mis à jour : jugé ancien.
        t.store.set_price_auto(holding.holding.id, 10.0);
        t.store.lock().holdings[0].price_updated_at = None;

        t.app.alerts.evaluate().await.unwrap();
        let alerts = t.app.alerts.list(false).await.unwrap();
        assert_eq!(
            kinds(&alerts),
            [AlertKind::ConnectionExpired, AlertKind::StalePrices]
        );

        t.store.lock().connections[0].status = ConnectionStatus::Active;
        let report = t.app.alerts.evaluate().await.unwrap();
        assert_eq!(report.resolved, 1);
        let alerts = t.app.alerts.list(false).await.unwrap();
        let expired = alerts
            .iter()
            .find(|alert| alert.kind == AlertKind::ConnectionExpired)
            .unwrap();
        assert!(!expired.is_active());
    }

    #[tokio::test]
    async fn read_archive_and_read_all() {
        let t = TestApp::household().await;
        livret(&t, 30_000.0).await;
        t.app.alerts.evaluate().await.unwrap();
        let id = t.app.alerts.list(false).await.unwrap()[0].id.as_uuid();

        let read = t
            .app
            .alerts
            .update(
                id,
                AlertPatch {
                    read: Some(true),
                    archived: None,
                },
            )
            .await
            .unwrap();
        assert!(read.read_at.is_some());
        let archived = AlertPatch {
            read: None,
            archived: Some(true),
        };
        t.app.alerts.update(id, archived).await.unwrap();
        assert!(t.app.alerts.list(false).await.unwrap().is_empty());
        assert_eq!(t.app.alerts.list(true).await.unwrap().len(), 1);

        // Archivée, la condition qui dure ne la fait pas réapparaître.
        t.clock.advance_days(1);
        t.app.alerts.evaluate().await.unwrap();
        assert!(t.app.alerts.list(false).await.unwrap().is_empty());

        let unread = AlertPatch {
            read: Some(false),
            archived: Some(false),
        };
        t.app.alerts.update(id, unread).await.unwrap();
        assert_eq!(t.app.alerts.read_all().await.unwrap(), 1);
        assert_eq!(t.app.alerts.read_all().await.unwrap(), 0);

        assert!(matches!(
            t.app
                .alerts
                .update(Uuid::new_v4(), AlertPatch::default())
                .await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn rules_are_configured_and_disabled() {
        let t = TestApp::household().await;
        livret(&t, 900.0).await;
        assert_eq!(
            t.app.alerts.rules().await.unwrap().len(),
            AlertRuleKind::ALL.len()
        );
        let lower_ceiling = RulePatch {
            ceiling: Some(1_000.0),
            ..RulePatch::default()
        };
        let rule = t
            .app
            .alerts
            .update_rule("savings_ceiling", lower_ceiling)
            .await
            .unwrap();
        assert_eq!(rule.ceiling(), Some(1_000.0));
        assert_eq!(t.app.alerts.evaluate().await.unwrap().created, 1);

        let disable = RulePatch {
            enabled: Some(false),
            ..RulePatch::default()
        };
        t.app
            .alerts
            .update_rule("savings_ceiling", disable)
            .await
            .unwrap();
        // Règle désactivée : la condition n'est plus observée, l'alerte est résolue.
        assert_eq!(t.app.alerts.evaluate().await.unwrap().resolved, 1);
        let stored = t.app.alerts.rules().await.unwrap();
        let ceiling = stored
            .iter()
            .find(|rule| rule.kind() == AlertRuleKind::SavingsCeiling)
            .unwrap();
        assert!(!ceiling.enabled());
        assert_eq!(ceiling.ceiling(), Some(1_000.0));

        let invalid = RulePatch {
            threshold_pct: Some(0.0),
            ..RulePatch::default()
        };
        assert!(matches!(
            t.app.alerts.update_rule("savings_ceiling", invalid).await,
            Err(AppError::Domain(_))
        ));
        assert!(matches!(
            t.app
                .alerts
                .update_rule("inconnue", RulePatch::default())
                .await,
            Err(AppError::NotFound(_))
        ));
    }
}
