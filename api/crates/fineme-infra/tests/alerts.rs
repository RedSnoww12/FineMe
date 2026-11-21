//! Tests d'intégration des alertes et de leurs règles contre un vrai Postgres (testcontainers).
//! Toutes les données sont fictives.

use chrono::{DateTime, TimeZone, Utc};
use fineme_application::ports::{AlertRepository, RepoError};
use fineme_domain::alert::{
    Alert, AlertCandidate, AlertKind, AlertRule, AlertRuleKind, AlertSeverity, reconcile,
};
use fineme_domain::ids::AlertId;
use fineme_infra::postgres::{self, PgStore};
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

async fn test_db() -> (PgStore, ContainerAsync<Postgres>) {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("démarrage du conteneur Postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("port du conteneur");
    let host = container.get_host().await.expect("hôte du conteneur");
    let pool = postgres::connect(&format!(
        "postgres://postgres:postgres@{host}:{port}/postgres"
    ))
    .await
    .unwrap();
    postgres::migrate(&pool).await.unwrap();
    (PgStore::new(pool), container)
}

fn at(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, day, 7, 0, 0).unwrap()
}

fn candidate(key: &str) -> AlertCandidate {
    AlertCandidate {
        key: key.into(),
        kind: AlertKind::SavingsCeiling,
        severity: AlertSeverity::Warning,
        title: "Plafond proche : Livret Fictif".into(),
        message: "« Livret Fictif » atteint 95 % de son plafond.".into(),
    }
}

#[tokio::test]
async fn alerts_round_trip_and_keep_their_state() {
    let (store, _container) = test_db().await;
    assert!(store.alerts().await.unwrap().is_empty());

    let created = reconcile(
        &[],
        vec![candidate("a"), candidate("b")],
        at(1),
        AlertId::generate,
    )
    .changed;
    store.save_alerts(&created).await.unwrap();
    let mut stored = store.alerts().await.unwrap();
    stored.sort_by(|x, y| x.key.cmp(&y.key));
    let mut expected = created.clone();
    expected.sort_by(|x, y| x.key.cmp(&y.key));
    assert_eq!(stored, expected);

    let mut first: Alert = stored[0].clone();
    first.set_read(true, at(2));
    first.set_archived(true, at(2));
    first.resolved_at = Some(at(3));
    store
        .save_alerts(std::slice::from_ref(&first))
        .await
        .unwrap();
    assert_eq!(store.find_alert(first.id).await.unwrap(), Some(first));
    assert_eq!(store.find_alert(AlertId::generate()).await.unwrap(), None);
}

#[tokio::test]
async fn a_condition_key_is_unique() {
    let (store, _container) = test_db().await;
    let first = reconcile(&[], vec![candidate("a")], at(1), AlertId::generate).changed;
    store.save_alerts(&first).await.unwrap();
    let duplicate = reconcile(&[], vec![candidate("a")], at(2), AlertId::generate).changed;
    assert_eq!(
        store.save_alerts(&duplicate).await,
        Err(RepoError::Conflict)
    );
    assert_eq!(store.alerts().await.unwrap().len(), 1);
}

#[tokio::test]
async fn rules_are_saved_and_replaced_per_kind() {
    let (store, _container) = test_db().await;
    assert!(store.rules().await.unwrap().is_empty());
    let rule = AlertRule::new(
        AlertRuleKind::SavingsCeiling,
        true,
        Some(80.0),
        Some(10_000.0),
    )
    .unwrap();
    store.save_rule(&rule).await.unwrap();
    let disabled = rule.updated(Some(false), None, None).unwrap();
    store.save_rule(&disabled).await.unwrap();
    let goals = AlertRule::new(AlertRuleKind::Goals, false, None, None).unwrap();
    store.save_rule(&goals).await.unwrap();
    assert_eq!(store.rules().await.unwrap(), vec![goals, disabled]);
}
