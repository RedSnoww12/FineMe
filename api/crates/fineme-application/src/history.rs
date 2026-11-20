//! Nettoyage de l'historique des instantanés : revue des points (suspects signalés), correction d'un point,
//! suppression d'une plage et remise à zéro. Le recalcul du jour passe par `OverviewService::record_snapshots`.

use std::sync::Arc;

use chrono::NaiveDate;
use fineme_domain::history::{
    AnomalyThreshold, DateRange, PointReview, corrected_totals, review_points,
};
use fineme_domain::ids::MemberId;
use fineme_domain::portfolio::Totals;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::overview::history_start;
use crate::ports::{Clock, PortfolioRepository, SnapshotPoint, SnapshotRepository};

/// Mot à saisir pour effacer tout l'historique.
pub const RESET_CONFIRMATION: &str = "EFFACER";
const RESET_NOT_CONFIRMED: &str =
    "Confirmation manquante : saisissez EFFACER pour supprimer tout l'historique";
const TODAY_NOT_CORRECTABLE: &str = "Le point du jour est recalculé à chaque instantané : corrigez plutôt le compte concerné, puis recalculez le point du jour";
const POINT_NOT_FOUND: &str = "Aucun point d'historique à cette date pour cette vue";

pub struct HistoryService {
    pub(crate) snapshots: Arc<dyn SnapshotRepository>,
    pub(crate) members: Arc<dyn PortfolioRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Point de l'historique et son diagnostic.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewedPoint {
    pub point: SnapshotPoint,
    pub change: Option<f64>,
    /// Variation anormale ; jamais vrai pour un point corrigé à la main.
    pub suspicious: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryReview {
    pub threshold: AnomalyThreshold,
    pub points: Vec<ReviewedPoint>,
}

fn reviewed(points: Vec<SnapshotPoint>, threshold: AnomalyThreshold) -> Vec<ReviewedPoint> {
    let totals: Vec<Totals> = points.iter().map(|point| point.totals).collect();
    let reviews = review_points(&totals, threshold);
    points
        .into_iter()
        .zip(reviews)
        .map(
            |(point, PointReview { change, suspicious })| ReviewedPoint {
                suspicious: suspicious && !point.corrected,
                change,
                point,
            },
        )
        .collect()
}

impl HistoryService {
    /// Le membre doit exister ; `None` = foyer.
    async fn view(&self, member_id: Option<Uuid>) -> AppResult<Option<MemberId>> {
        let member = member_id.map(MemberId::from_uuid);
        if let Some(id) = member
            && !self.members.members().await?.iter().any(|m| m.id == id)
        {
            return Err(AppError::NotFound("Membre introuvable"));
        }
        Ok(member)
    }

    /// Tous les points d'une vue, du plus ancien au plus récent.
    async fn all_points(&self, member: Option<MemberId>) -> AppResult<Vec<SnapshotPoint>> {
        let since = history_start(None, self.clock.today())?;
        Ok(self.snapshots.since(member, since).await?)
    }

    /// Tous les points de la vue, du plus ancien au plus récent, avec les points suspects signalés.
    pub async fn review(
        &self,
        member_id: Option<Uuid>,
        threshold: Option<f64>,
    ) -> AppResult<HistoryReview> {
        let threshold = threshold
            .map(AnomalyThreshold::new)
            .transpose()?
            .unwrap_or_default();
        let member = self.view(member_id).await?;
        let points = self.all_points(member).await?;
        Ok(HistoryReview {
            threshold,
            points: reviewed(points, threshold),
        })
    }

    /// Corrige à la main les totaux d'un jour passé pour une vue ; renvoie le point corrigé.
    pub async fn correct(
        &self,
        date: NaiveDate,
        member_id: Option<Uuid>,
        gross_assets: f64,
        liabilities: f64,
    ) -> AppResult<ReviewedPoint> {
        let totals = corrected_totals(gross_assets, liabilities)?;
        if date >= self.clock.today() {
            return Err(AppError::invalid(TODAY_NOT_CORRECTABLE));
        }
        let member = self.view(member_id).await?;
        if !self.snapshots.correct(date, member, totals).await? {
            return Err(AppError::NotFound(POINT_NOT_FOUND));
        }
        let points = self.all_points(member).await?;
        reviewed(points, AnomalyThreshold::default())
            .into_iter()
            .find(|reviewed| reviewed.point.date == date)
            .ok_or(AppError::NotFound(POINT_NOT_FOUND))
    }

    /// Supprime les points de `from` à `to` inclus, pour toutes les vues ; renvoie le nombre de jours supprimés.
    pub async fn delete(&self, from: NaiveDate, to: NaiveDate) -> AppResult<u64> {
        let range = DateRange::new(from, to)?;
        Ok(self.snapshots.delete_range(range).await?)
    }

    /// Efface tout l'historique si `confirmation` vaut exactement `EFFACER` (espaces ignorés).
    pub async fn reset(&self, confirmation: Option<&str>) -> AppResult<u64> {
        if confirmation.map(str::trim) != Some(RESET_CONFIRMATION) {
            return Err(AppError::invalid(RESET_NOT_CONFIRMED));
        }
        Ok(self.snapshots.delete_all().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::Clock;
    use crate::testing::TestApp;
    use fineme_domain::portfolio::Snapshot;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn snapshot(gross_assets: f64, liabilities: f64) -> Snapshot {
        Snapshot {
            totals: Totals {
                gross_assets,
                liabilities,
                net_worth: gross_assets - liabilities,
            },
            accounts: vec![],
        }
    }

    /// Foyer : 5 jours passés dont un pic aberrant le 3e ; Bob : les mêmes jours, sans pic.
    async fn seeded() -> (TestApp, Vec<NaiveDate>) {
        let t = TestApp::household().await;
        let today = t.clock.today();
        let days: Vec<NaiveDate> = (1..=5)
            .rev()
            .map(|n| today - chrono::Duration::days(n))
            .collect();
        let household = [100_000.0, 101_000.0, 10_000.0, 102_000.0, 102_500.0];
        for (day, gross) in days.iter().zip(household) {
            t.store
                .upsert(*day, None, &snapshot(gross, 0.0))
                .await
                .unwrap();
            t.store
                .upsert(*day, Some(t.bob), &snapshot(40_000.0, 0.0))
                .await
                .unwrap();
        }
        (t, days)
    }

    #[tokio::test]
    async fn review_flags_the_spike_in_the_requested_view() {
        let (t, days) = seeded().await;
        let review = t.app.history.review(None, None).await.unwrap();
        assert_eq!(review.threshold, AnomalyThreshold::default());
        let flagged: Vec<NaiveDate> = review
            .points
            .iter()
            .filter(|p| p.suspicious)
            .map(|p| p.point.date)
            .collect();
        assert_eq!(flagged, vec![days[2]]);
        assert_eq!(review.points[0].change, None);
        assert!(close(review.points[1].change.unwrap(), 0.01));

        let bob = t
            .app
            .history
            .review(Some(t.bob.as_uuid()), None)
            .await
            .unwrap();
        assert_eq!(bob.points.len(), 5);
        assert!(bob.points.iter().all(|p| !p.suspicious));
    }

    #[tokio::test]
    async fn review_validates_threshold_and_member() {
        let (t, _) = seeded().await;
        // Seuil de 1 000 % : la chute de 90 % n'est plus signalée.
        let loose = t.app.history.review(None, Some(10.0)).await.unwrap();
        assert!(loose.points.iter().all(|p| !p.suspicious));
        assert!(matches!(
            t.app.history.review(None, Some(0.0)).await,
            Err(AppError::Domain(_))
        ));
        assert!(matches!(
            t.app.history.review(Some(Uuid::new_v4()), None).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn correcting_a_point_replaces_its_totals_in_that_view_only() {
        let (t, days) = seeded().await;
        let corrected = t
            .app
            .history
            .correct(days[2], None, 101_500.0, 1_000.0)
            .await
            .unwrap();
        assert!(corrected.point.corrected);
        assert!(close(corrected.point.totals.net_worth, 100_500.0));
        assert_eq!(corrected.point.accounts, None);
        assert!(!corrected.suspicious);

        let review = t.app.history.review(None, None).await.unwrap();
        assert!(review.points.iter().all(|p| !p.suspicious));
        // La vue de Bob n'a pas bougé.
        let bob = t
            .app
            .overview
            .history(None, Some(t.bob.as_uuid()))
            .await
            .unwrap();
        assert!(
            bob.iter()
                .all(|p| !p.corrected && close(p.totals.net_worth, 40_000.0))
        );
        // Le graphique du foyer lit la valeur corrigée.
        let household = t.app.overview.history(None, None).await.unwrap();
        assert!(close(household[2].totals.net_worth, 100_500.0));
    }

    #[tokio::test]
    async fn correction_is_refused_for_today_unknown_days_and_invalid_amounts() {
        let (t, days) = seeded().await;
        t.app.overview.record_snapshots().await.unwrap();
        let today = t.clock.today();
        assert!(matches!(
            t.app.history.correct(today, None, 1.0, 0.0).await,
            Err(AppError::Invalid(_))
        ));
        let missing = days[0] - chrono::Duration::days(10);
        assert!(matches!(
            t.app.history.correct(missing, None, 1.0, 0.0).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.history.correct(days[0], None, -1.0, 0.0).await,
            Err(AppError::Domain(_))
        ));
        assert!(matches!(
            t.app
                .history
                .correct(days[0], Some(Uuid::new_v4()), 1.0, 0.0)
                .await,
            Err(AppError::NotFound(_))
        ));
        // Rien n'a été corrigé.
        assert!(t.store.state().snapshots.iter().all(|(_, p)| !p.corrected));
    }

    #[tokio::test]
    async fn deleting_a_range_removes_those_days_in_every_view() {
        let (t, days) = seeded().await;
        assert_eq!(t.app.history.delete(days[2], days[2]).await.unwrap(), 1);
        assert_eq!(t.app.history.delete(days[3], days[4]).await.unwrap(), 2);
        // Plage déjà vide : rien à supprimer.
        assert_eq!(t.app.history.delete(days[2], days[4]).await.unwrap(), 0);
        let remaining = |member| {
            let t = &t;
            async move {
                t.app
                    .overview
                    .history(None, member)
                    .await
                    .unwrap()
                    .iter()
                    .map(|p| p.date)
                    .collect::<Vec<_>>()
            }
        };
        assert_eq!(remaining(None).await, vec![days[0], days[1]]);
        assert_eq!(
            remaining(Some(t.bob.as_uuid())).await,
            vec![days[0], days[1]]
        );
        assert!(matches!(
            t.app.history.delete(days[4], days[0]).await,
            Err(AppError::Domain(_))
        ));
    }

    #[tokio::test]
    async fn reset_requires_the_exact_confirmation_word() {
        let (t, _) = seeded().await;
        for word in [None, Some(""), Some("effacer"), Some("OUI")] {
            assert!(
                matches!(t.app.history.reset(word).await, Err(AppError::Invalid(_))),
                "{word:?}"
            );
        }
        assert_eq!(t.store.state().snapshots.len(), 10);
        assert_eq!(t.app.history.reset(Some(" EFFACER ")).await.unwrap(), 5);
        assert!(t.store.state().snapshots.is_empty());
    }

    #[tokio::test]
    async fn todays_point_is_recomputed_from_current_data() {
        let (t, _) = seeded().await;
        t.checking(&[(t.alice, 1.0)], 2_500.0).await;
        assert_eq!(
            t.app.overview.record_snapshots().await.unwrap(),
            t.clock.today()
        );
        let review = t.app.history.review(None, None).await.unwrap();
        let last = review.points.last().unwrap();
        assert_eq!(last.point.date, t.clock.today());
        assert!(close(last.point.totals.net_worth, 2_500.0));
        // 102 500 € → 2 500 € : signalé, la donnée du jour est à vérifier.
        assert!(last.suspicious);
    }
}
