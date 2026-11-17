//! Tâche quotidienne, déclenchée par un planificateur externe : cours du matin et instantané du jour,
//! même quand personne n'ouvre l'application.

use chrono::NaiveDate;

use crate::app::App;
use crate::error::{AppError, AppResult};
use crate::prices::RefreshReport;
use crate::secret::code_matches;

/// Longueur minimale du secret : la route de la tâche est appelable sans session.
pub const MIN_CRON_SECRET_LEN: usize = 16;
const DAILY_DISABLED: &str = "La tâche quotidienne n'est pas activée sur ce serveur (CRON_SECRET)";
const INVALID_CRON_SECRET: &str = "Secret de la tâche quotidienne invalide ou manquant";

#[derive(Debug, Clone, PartialEq)]
pub struct DailyReport {
    pub prices: RefreshReport,
    pub snapshot_date: NaiveDate,
}

impl App {
    /// Vrai si la tâche quotidienne est activée (`CRON_SECRET` défini).
    pub fn daily_enabled(&self) -> bool {
        self.cron_secret.is_some()
    }

    /// Vérifie le secret, rafraîchit les taux et les cours, puis enregistre l'instantané du jour
    /// (qui valorise donc le patrimoine aux cours tout juste récupérés). Idempotente sur une journée.
    pub async fn run_daily(&self, provided_secret: Option<&str>) -> AppResult<DailyReport> {
        let expected = self
            .cron_secret
            .as_deref()
            .ok_or(AppError::NotConfigured(DAILY_DISABLED))?;
        if !code_matches(expected, provided_secret) {
            return Err(AppError::Forbidden(INVALID_CRON_SECRET));
        }
        let prices = self.prices.refresh().await?;
        let snapshot_date = self.overview.record_snapshots().await?;
        Ok(DailyReport {
            prices,
            snapshot_date,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::HoldingInput;
    use crate::ports::Clock;
    use crate::testing::{TestApp, holding_input};

    const SECRET: &str = "secret-quotidien-fictif";

    #[tokio::test]
    async fn daily_run_refreshes_prices_then_records_valued_snapshots() {
        let t = TestApp::with_cron_secret(SECRET).await;
        let pea = t.investment(t.alice, 0.0).await;
        let input = HoldingInput {
            symbol: Some(Some("DEMO".into())),
            ..holding_input("ETF Démo", 10.0, Some(20.0), 20.0)
        };
        t.app.holdings.create(pea.as_uuid(), &input).await.unwrap();
        t.securities.set("DEMO", 30.0, "EUR");

        let report = t.app.run_daily(Some(SECRET)).await.unwrap();
        assert_eq!(report.prices.updated, 1);
        assert_eq!(report.snapshot_date, t.clock.today());
        let household = t.app.overview.history(None, None).await.unwrap();
        // L'instantané est pris après la mise à jour des cours : 10 × 30 €.
        assert_eq!(household.len(), 1);
        assert!((household[0].totals.net_worth - 300.0).abs() < 1e-9);
        // Foyer + Alice + Bob.
        assert_eq!(t.store.state().snapshots.len(), 3);

        // Rejouée le même jour, la tâche remplace l'instantané au lieu d'en ajouter un.
        t.app.run_daily(Some(SECRET)).await.unwrap();
        assert_eq!(t.store.state().snapshots.len(), 3);
    }

    #[tokio::test]
    async fn daily_run_requires_the_secret() {
        let t = TestApp::with_cron_secret(SECRET).await;
        assert!(t.app.daily_enabled());
        for provided in [None, Some(""), Some("secret-quotidien-faux!")] {
            let error = t.app.run_daily(provided).await.unwrap_err();
            assert!(matches!(error, AppError::Forbidden(_)), "{provided:?}");
        }
        // Rien n'a été enregistré.
        assert!(t.store.state().snapshots.is_empty());
    }

    #[tokio::test]
    async fn daily_run_is_unavailable_without_a_configured_secret() {
        let t = TestApp::household().await;
        assert!(!t.app.daily_enabled());
        assert!(matches!(
            t.app.run_daily(Some(SECRET)).await,
            Err(AppError::NotConfigured(_))
        ));
    }
}
