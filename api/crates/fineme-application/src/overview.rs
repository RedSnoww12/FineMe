use std::sync::Arc;

use chrono::{Months, NaiveDate};
use fineme_domain::ids::MemberId;
use fineme_domain::member::Member;
use fineme_domain::ownership::View;
use fineme_domain::portfolio::{Overview, Portfolio};
use fineme_domain::timeline::{self, Comparison, Delta, NetWorthChange, Period, SnapshotRef};
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

fn snapshot_ref(point: &SnapshotPoint) -> SnapshotRef<'_> {
    SnapshotRef {
        totals: &point.totals,
        accounts: point.accounts.as_deref(),
    }
}

/// Vue d'ensemble d'une vue et variation de son patrimoine net sur une période, calculées sans rien enregistrer.
#[derive(Debug, Clone, PartialEq)]
pub struct OverviewSummary {
    pub as_of: NaiveDate,
    pub view: View,
    /// Nom du membre de la vue ; `None` pour le foyer.
    pub member_name: Option<String>,
    pub period: Period,
    pub overview: Overview,
    /// `None` : aucun instantané dans l'historique de la vue.
    pub change: Option<NetWorthChange>,
}

/// Instantané retenu pour une date demandée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotDate {
    pub requested: NaiveDate,
    pub date: NaiveDate,
}

/// Patrimoine à deux dates et leurs écarts.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryComparison {
    pub first_date: Option<NaiveDate>,
    pub a: Option<SnapshotDate>,
    pub b: Option<SnapshotDate>,
    /// `None` si l'une des deux dates n'a pas d'instantané.
    pub comparison: Option<Comparison>,
    /// Patrimoine net de chaque membre ; vue foyer seulement, et seulement si `comparison` existe.
    pub members: Option<Vec<Delta<Member>>>,
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

    /// Vue d'ensemble et variation sur la période, sans enregistrer d'instantané (lecture seule).
    pub async fn summary(
        &self,
        member_id: Option<Uuid>,
        period: Period,
    ) -> AppResult<OverviewSummary> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        let overview = portfolio.overview(view);
        let today = self.clock.today();
        let change = self
            .period_start(view.member(), period.start(today))
            .await?
            .map(|start| {
                NetWorthChange::new(
                    start.date,
                    start.totals.net_worth,
                    overview.totals.net_worth,
                )
            });
        Ok(OverviewSummary {
            as_of: today,
            view,
            member_name: view
                .member()
                .and_then(|id| portfolio.member(id))
                .map(|member| member.name.clone()),
            period,
            overview,
            change,
        })
    }

    /// Instantané de départ d'une période : le dernier au plus tard à `start`, sinon le premier après ;
    /// le premier de l'historique si la période n'a pas de début.
    async fn period_start(
        &self,
        member: Option<MemberId>,
        start: Option<NaiveDate>,
    ) -> AppResult<Option<SnapshotPoint>> {
        if let Some(start) = start
            && let Some(point) = self.snapshots.at(member, start).await?
        {
            return Ok(Some(point));
        }
        Ok(self.snapshots.first(member).await?)
    }

    /// Patrimoine de la vue à deux dates (`b` absente = `a`) et leurs écarts ; lecture seule.
    pub async fn compare(
        &self,
        member_id: Option<Uuid>,
        a: NaiveDate,
        b: Option<NaiveDate>,
    ) -> AppResult<HistoryComparison> {
        let b = b.unwrap_or(a);
        let members = self.members.members().await?;
        let member = member_id.map(MemberId::from_uuid);
        if let Some(id) = member
            && !members.iter().any(|m| m.id == id)
        {
            return Err(AppError::NotFound("Membre introuvable"));
        }
        let first_date = self.snapshots.first(member).await?.map(|point| point.date);
        let first = self.snapshots.at(member, a).await?;
        let second = self.snapshots.at(member, b).await?;
        let reference = |requested, point: Option<&SnapshotPoint>| {
            point.map(|point| SnapshotDate {
                requested,
                date: point.date,
            })
        };
        let (Some(first), Some(second)) = (first.as_ref(), second.as_ref()) else {
            return Ok(HistoryComparison {
                first_date,
                a: reference(a, first.as_ref()),
                b: reference(b, second.as_ref()),
                comparison: None,
                members: None,
            });
        };
        let comparison = timeline::compare(snapshot_ref(first), snapshot_ref(second));
        let members = match member {
            Some(_) => None,
            None => Some(self.member_deltas(members, a, b).await?),
        };
        Ok(HistoryComparison {
            first_date,
            a: reference(a, Some(first)),
            b: reference(b, Some(second)),
            comparison: Some(comparison),
            members,
        })
    }

    /// Patrimoine net de chaque membre aux deux dates (0 sans instantané).
    async fn member_deltas(
        &self,
        members: Vec<Member>,
        a: NaiveDate,
        b: NaiveDate,
    ) -> AppResult<Vec<Delta<Member>>> {
        let mut rows = Vec::with_capacity(members.len());
        for member in members {
            let net = |point: Option<SnapshotPoint>| point.map_or(0.0, |p| p.totals.net_worth);
            let from = net(self.snapshots.at(Some(member.id), a).await?);
            let to = net(self.snapshots.at(Some(member.id), b).await?);
            rows.push(Delta::new(member, from, to));
        }
        Ok(rows)
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
    async fn summary_records_nothing_and_measures_the_change_since_the_period_start() {
        let t = seeded().await;
        let first = t
            .app
            .overview
            .summary(None, Period::OneMonth)
            .await
            .unwrap();
        assert_eq!(first.change, None);
        assert_eq!(first.member_name, None);
        assert!(t.store.state().snapshots.is_empty(), "lecture seule");

        let start = t.app.overview.record_snapshots().await.unwrap();
        t.clock.advance_days(40);
        t.checking(&[(t.alice, 1.0)], 450.0).await;
        let summary = t
            .app
            .overview
            .summary(None, Period::OneMonth)
            .await
            .unwrap();
        let change = summary.change.unwrap();
        assert_eq!(change.from_date, start);
        assert!(close(change.net_worth_start, 101_550.0));
        assert!(close(change.delta, 450.0));
        assert!(close(summary.overview.totals.net_worth, 102_000.0));
        assert_eq!(summary.period, Period::OneMonth);
        assert_eq!(
            t.store.state().snapshots.len(),
            3,
            "foyer + 2 membres, une seule fois"
        );
    }

    #[tokio::test]
    async fn summary_of_a_member_names_it_and_unknown_members_are_refused() {
        let t = seeded().await;
        let alice = t
            .app
            .overview
            .summary(Some(t.alice.as_uuid()), Period::All)
            .await
            .unwrap();
        assert_eq!(alice.member_name.as_deref(), Some("Alice"));
        assert_eq!(alice.view, View::Member(t.alice));
        let unknown = t
            .app
            .overview
            .summary(Some(Uuid::new_v4()), Period::All)
            .await;
        assert!(matches!(unknown, Err(AppError::NotFound(_))));
    }

    #[tokio::test]
    async fn comparison_reads_the_snapshots_in_force_at_both_dates() {
        let t = seeded().await;
        let first = t.app.overview.record_snapshots().await.unwrap();
        t.clock.advance_days(10);
        t.checking(&[(t.alice, 1.0)], 450.0).await;
        let second = t.app.overview.record_snapshots().await.unwrap();

        let before_history = first - chrono::Duration::days(1);
        let missing = t
            .app
            .overview
            .compare(None, before_history, Some(second))
            .await
            .unwrap();
        assert_eq!(missing.first_date, Some(first));
        assert_eq!(missing.a, None);
        assert_eq!(missing.comparison, None);
        assert_eq!(missing.members, None);

        // Un jour sans instantané retient le précédent ; une date future, le dernier.
        let between = first + chrono::Duration::days(3);
        let future = second + chrono::Duration::days(100);
        let result = t
            .app
            .overview
            .compare(None, between, Some(future))
            .await
            .unwrap();
        assert_eq!(
            result.a,
            Some(SnapshotDate {
                requested: between,
                date: first
            })
        );
        assert_eq!(result.b.map(|b| b.date), Some(second));
        let comparison = result.comparison.unwrap();
        assert!(close(comparison.totals[2].delta, 450.0));
        let members = result.members.unwrap();
        assert_eq!(members.len(), 2);
        let alice = members.iter().find(|row| row.key.id == t.alice).unwrap();
        assert!(close(alice.delta, 450.0));

        let single = t.app.overview.compare(None, second, None).await.unwrap();
        assert_eq!(single.b.map(|b| b.date), Some(second));
        assert!(
            single
                .comparison
                .unwrap()
                .totals
                .iter()
                .all(|row| row.delta == 0.0)
        );

        let of_alice = t
            .app
            .overview
            .compare(Some(t.alice.as_uuid()), first, Some(second))
            .await
            .unwrap();
        assert_eq!(of_alice.members, None);
        let unknown = t
            .app
            .overview
            .compare(Some(Uuid::new_v4()), first, None)
            .await;
        assert!(matches!(unknown, Err(AppError::NotFound(_))));
        assert!(close(of_alice.comparison.unwrap().totals[2].delta, 450.0));
        assert_eq!(
            t.store.state().snapshots.len(),
            6,
            "la comparaison n'écrit rien"
        );
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
