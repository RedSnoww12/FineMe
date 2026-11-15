use fineme_domain::ids::AccountId;
use fineme_domain::loan::ScheduleRow;
use fineme_domain::portfolio::{LoanView, RealEstateReport};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::reader::{PortfolioReader, view_for};

pub struct RealEstateService {
    pub(crate) reader: PortfolioReader,
}

impl RealEstateService {
    pub async fn report(&self, member_id: Option<Uuid>) -> AppResult<RealEstateReport> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        Ok(portfolio.real_estate(view))
    }

    /// Tableau d'amortissement du prêt porté par un compte.
    pub async fn loan_schedule(&self, account_id: Uuid) -> AppResult<(LoanView, Vec<ScheduleRow>)> {
        let portfolio = self.reader.load().await?;
        let view = portfolio
            .account(AccountId::from_uuid(account_id))
            .and_then(|account| portfolio.loan_view(account))
            .ok_or(AppError::NotFound("Prêt introuvable"))?;
        let rows = view.loan.terms.schedule();
        Ok((view, rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestApp;

    #[tokio::test]
    async fn report_links_loans_and_weights_by_view() {
        let t = TestApp::household().await;
        t.house_with_loan().await;
        let household = t.app.real_estate.report(None).await.unwrap();
        assert_eq!(household.items.len(), 1);
        assert_eq!(household.items[0].loans.len(), 1);
        assert_eq!(household.totals.equity, 100_000.0);
        let alice = t
            .app
            .real_estate
            .report(Some(t.alice.as_uuid()))
            .await
            .unwrap();
        assert_eq!(alice.totals.value, 150_000.0);
        assert_eq!(alice.totals.remaining_debt, 100_000.0);
        assert_eq!(alice.totals.monthly_rent, 600.0);
        assert!(matches!(
            t.app.real_estate.report(Some(Uuid::new_v4())).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn report_is_empty_without_property() {
        let t = TestApp::household().await;
        t.checking(&[(t.alice, 1.0)], 10.0).await;
        let report = t.app.real_estate.report(None).await.unwrap();
        assert!(report.items.is_empty());
        assert_eq!(report.totals.value, 0.0);
    }

    #[tokio::test]
    async fn schedule_for_loan_account_only() {
        let t = TestApp::household().await;
        let (house, loan) = t.house_with_loan().await;
        let (view, rows) = t
            .app
            .real_estate
            .loan_schedule(loan.as_uuid())
            .await
            .unwrap();
        assert_eq!(rows.len(), 240);
        assert_eq!(rows[239].remaining, 0.0);
        assert_eq!(view.summary.end_date, rows[239].date);
        assert!(matches!(
            t.app.real_estate.loan_schedule(house.as_uuid()).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.real_estate.loan_schedule(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
    }
}
