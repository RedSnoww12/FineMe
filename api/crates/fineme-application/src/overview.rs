use std::sync::Arc;

use chrono::{Months, NaiveDate};
use fineme_domain::ids::MemberId;
use fineme_domain::ownership::View;
use fineme_domain::portfolio::{Overview, Portfolio};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{Clock, PortfolioRepository, SnapshotPoint, SnapshotRepository};
use crate::reader::{PortfolioReader, view_for};

pub struct OverviewService {
    pub(crate) reader: PortfolioReader,
    pub(crate) members: Arc<dyn PortfolioRepository>,
    pub(crate) snapshots: Arc<dyn SnapshotRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Début de la période d'historique : `1m`, `3m`, `1y` ou `all` (défaut).
pub(crate) fn history_start(range: Option<&str>, today: NaiveDate) -> AppResult<NaiveDate> {
    let months = match range.unwrap_or("all") {
        "1m" => Some(1),
        "3m" => Some(3),
        "1y" => Some(12),
        "all" => None,
        _ => return Err(AppError::invalid("Période invalide (1m, 3m, 1y ou all)")),
    };
    // « all » : borne basse fixe, compatible avec la plage de dates de Postgres.
    Ok(months
        .and_then(|m| today.checked_sub_months(Months::new(m)))
        .or(NaiveDate::from_ymd_opt(1900, 1, 1))
        .unwrap_or(today))
}

impl OverviewService {
    /// Vue d'ensemble ; enregistre au passage l'instantané du jour (foyer + chaque membre).
    pub async fn overview(&self, member_id: Option<Uuid>) -> AppResult<Overview> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        self.save_snapshots(&portfolio).await?;
        Ok(portfolio.overview(view))
    }

    /// Enregistre (ou remplace) l'instantané du jour, foyer et chaque membre ; renvoie sa date.
    pub async fn record_snapshots(&self) -> AppResult<NaiveDate> {
        let portfolio = self.reader.load().await?;
        self.save_snapshots(&portfolio).await?;
        Ok(portfolio.today)
    }

    async fn save_snapshots(&self, portfolio: &Portfolio) -> AppResult<()> {
        let views = std::iter::once(View::Household)
            .chain(portfolio.members.iter().map(|m| View::Member(m.id)));
        for view in views {
            self.snapshots
                .upsert(portfolio.today, view.member(), &portfolio.snapshot(view))
                .await?;
        }
        Ok(())
    }

    pub async fn history(
        &self,
        range: Option<&str>,
        member_id: Option<Uuid>,
    ) -> AppResult<Vec<SnapshotPoint>> {
        let since = history_start(range, self.clock.today())?;
        let member = member_id.map(MemberId::from_uuid);
        if let Some(id) = member
            && !self.members.members().await?.iter().any(|m| m.id == id)
        {
            return Err(AppError::NotFound("Membre introuvable"));
        }
        Ok(self.snapshots.since(member, since).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestApp, holding_input};
    use fineme_domain::account::AccountKind;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    /// Courant commun 60/20 (1 000 €), PEA d'Alice (500 € + 10 × 25 €), maison et prêt 50/50.
    async fn seeded() -> TestApp {
        let t = TestApp::household().await;
        t.checking(&[(t.alice, 0.6), (t.bob, 0.2)], 1000.0).await;
        let pea = t.investment(t.alice, 500.0).await;
        t.app
            .holdings
            .create(pea.as_uuid(), &holding_input("ETF", 10.0, Some(20.0), 25.0))
            .await
            .unwrap();
        t.house_with_loan().await;
        t
    }

    #[tokio::test]
    async fn household_overview_sums_all_shares() {
        let t = seeded().await;
        let o = t.app.overview.overview(None).await.unwrap();
        assert!(close(o.totals.gross_assets, 800.0 + 750.0 + 300_000.0));
        assert!(close(o.totals.liabilities, 200_000.0));
        assert!(close(o.totals.net_worth, 101_550.0));
        assert!(close(o.financial_assets, 750.0));
        assert!(close(o.invested, 200.0));
        assert!(close(o.gain, 50.0));
        assert_eq!(o.members.len(), 2);
    }

    #[tokio::test]
    async fn member_overview_uses_member_share_only() {
        let t = seeded().await;
        let alice = t
            .app
            .overview
            .overview(Some(t.alice.as_uuid()))
            .await
            .unwrap();
        let bob = t
            .app
            .overview
            .overview(Some(t.bob.as_uuid()))
            .await
            .unwrap();
        assert!(close(alice.totals.gross_assets, 600.0 + 750.0 + 150_000.0));
        assert!(close(bob.totals.gross_assets, 200.0 + 150_000.0));
        assert!(close(bob.totals.liabilities, 100_000.0));
        assert_eq!(bob.financial_assets, 0.0);
        let household = t.app.overview.overview(None).await.unwrap();
        assert!(close(
            alice.totals.net_worth + bob.totals.net_worth,
            household.totals.net_worth
        ));
        assert!(matches!(
            t.app.overview.overview(Some(Uuid::new_v4())).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn each_overview_call_upserts_todays_snapshots() {
        let t = seeded().await;
        t.app
            .overview
            .overview(Some(t.bob.as_uuid()))
            .await
            .unwrap();
        t.app.overview.overview(None).await.unwrap();
        // Un instantané par vue (foyer, Alice, Bob), quel que soit le filtre demandé.
        assert_eq!(t.store.state().snapshots.len(), 3);
        let household = t.app.overview.history(None, None).await.unwrap();
        assert_eq!(household.len(), 1);
        assert_eq!(household[0].date, t.clock.today());
        assert!(close(household[0].totals.net_worth, 101_550.0));
        let bob = t
            .app
            .overview
            .history(Some("all"), Some(t.bob.as_uuid()))
            .await
            .unwrap();
        assert!(close(bob[0].totals.net_worth, 50_200.0));
    }

    #[tokio::test]
    async fn history_details_each_account_held_in_the_view() {
        let t = seeded().await;
        t.app.overview.overview(None).await.unwrap();
        let household = t.app.overview.history(None, None).await.unwrap();
        let accounts = household[0].accounts.as_ref().unwrap();
        // Courant, PEA, maison, prêt.
        assert_eq!(accounts.len(), 4);
        let debts: f64 = accounts
            .iter()
            .filter(|a| a.kind.is_liability())
            .map(|a| a.value)
            .sum();
        assert!(close(debts, household[0].totals.liabilities));
        let gain: f64 = accounts.iter().filter_map(|a| a.gain).sum();
        assert!(close(gain, 50.0));

        let bob = t
            .app
            .overview
            .history(None, Some(t.bob.as_uuid()))
            .await
            .unwrap();
        let bob_accounts = bob[0].accounts.as_ref().unwrap();
        // Bob ne détient pas le PEA d'Alice.
        assert_eq!(bob_accounts.len(), 3);
        assert!(bob_accounts.iter().all(|a| a.gain.is_none()));
        let checking = bob_accounts
            .iter()
            .find(|a| a.kind == AccountKind::Checking)
            .unwrap();
        assert!(close(checking.value, 200.0));
    }

    #[tokio::test]
    async fn record_snapshots_saves_every_view_without_reading_the_overview() {
        let t = seeded().await;
        assert_eq!(
            t.app.overview.record_snapshots().await.unwrap(),
            t.clock.today()
        );
        assert_eq!(t.store.state().snapshots.len(), 3);
        let household = t.app.overview.history(None, None).await.unwrap();
        assert!(close(household[0].totals.net_worth, 101_550.0));
    }

    #[tokio::test]
    async fn snapshot_of_the_day_is_replaced_and_history_grows_over_days() {
        let t = seeded().await;
        t.app.overview.overview(None).await.unwrap();
        t.checking(&[(t.alice, 1.0)], 1000.0).await;
        t.app.overview.overview(None).await.unwrap();
        let points = t.app.overview.history(None, None).await.unwrap();
        assert_eq!(points.len(), 1);
        assert!(close(points[0].totals.net_worth, 102_550.0));
        t.clock.advance_days(1);
        t.app.overview.overview(None).await.unwrap();
        let points = t.app.overview.history(None, None).await.unwrap();
        assert_eq!(points.len(), 2);
        assert!(points[0].date < points[1].date);
    }

    #[tokio::test]
    async fn history_range_filters_old_points() {
        let t = seeded().await;
        t.app.overview.overview(None).await.unwrap();
        t.clock.advance_days(45);
        t.app.overview.overview(None).await.unwrap();
        assert_eq!(
            t.app
                .overview
                .history(Some("1m"), None)
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            t.app
                .overview
                .history(Some("3m"), None)
                .await
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            t.app
                .overview
                .history(Some("1y"), None)
                .await
                .unwrap()
                .len(),
            2
        );
        assert!(matches!(
            t.app.overview.history(Some("2y"), None).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app.overview.history(None, Some(Uuid::new_v4())).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn history_start_bounds() {
        let today = NaiveDate::from_ymd_opt(2026, 3, 31).unwrap();
        assert_eq!(
            history_start(Some("1m"), today).unwrap(),
            NaiveDate::from_ymd_opt(2026, 2, 28).unwrap()
        );
        assert_eq!(
            history_start(None, today).unwrap(),
            NaiveDate::from_ymd_opt(1900, 1, 1).unwrap()
        );
    }
}
