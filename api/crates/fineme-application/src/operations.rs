//! Journal d'opérations des comptes d'investissement (achats, ventes, dividendes, frais, versements, retraits).
//!
//! Cohabitation avec la saisie manuelle : une position qui a au moins un achat ou une vente dans le
//! journal voit sa quantité et son PRU recalculés à chaque modification du journal ; une position sans
//! achat ni vente garde sa saisie manuelle. Le journal ne modifie jamais le solde espèces du compte.

use std::sync::Arc;

use fineme_domain::account::Account;
use fineme_domain::holding::Holding;
use fineme_domain::ids::{HoldingId, OperationId};
use fineme_domain::operation::{
    Ledger, Movement, MovementFields, Operation, OperationKind, YearSummary, chronological,
};
use fineme_domain::portfolio::Portfolio;
use uuid::Uuid;

use crate::commands::{OperationInput, clean};
use crate::error::{AppError, AppResult};
use crate::holdings::positions_account;
use crate::ports::{Clock, OperationChange, OperationRepository, PositionUpdate};
use crate::reader::PortfolioReader;

/// Opération accompagnée de ce qui se déduit du journal.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationView {
    pub operation: Operation,
    /// Nom de la position concernée, s'il y en a une.
    pub holding_name: Option<String>,
    /// Plus-value réalisée (vente uniquement), dans la devise de l'opération.
    pub realized_gain: Option<f64>,
}

#[derive(Clone)]
pub struct OperationService {
    pub(crate) reader: PortfolioReader,
    pub(crate) operations: Arc<dyn OperationRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

fn holding_of<'a>(
    portfolio: &'a Portfolio,
    account: &Account,
    id: HoldingId,
) -> Option<&'a Holding> {
    portfolio
        .holdings_of(account.id)
        .iter()
        .find(|holding| holding.id == id)
}

fn view(portfolio: &Portfolio, ledger: &Ledger, operation: Operation) -> OperationView {
    let holding_name = operation.movement.holding().and_then(|id| {
        portfolio
            .holdings
            .values()
            .flatten()
            .find(|holding| holding.id == id)
            .map(|holding| holding.name.clone())
    });
    OperationView {
        realized_gain: ledger.realized_gain(operation.id),
        holding_name,
        operation,
    }
}

/// Positions dont la quantité et le PRU dépendent des opérations modifiées.
fn affected(operations: &[&Operation]) -> Vec<HoldingId> {
    let mut holdings: Vec<HoldingId> = operations
        .iter()
        .filter_map(|operation| operation.traded_holding())
        .collect();
    holdings.sort_unstable();
    holdings.dedup();
    holdings
}

impl OperationService {
    /// Construit l'opération demandée ; `existing` conserve son identifiant et son ordre de saisie.
    fn build(
        &self,
        portfolio: &Portfolio,
        account: &Account,
        input: &OperationInput,
        existing: Option<&Operation>,
    ) -> AppResult<Operation> {
        let kind = OperationKind::parse(input.kind.trim())?;
        if input.date > self.clock.today() {
            return Err(AppError::invalid(
                "La date de l'opération ne peut pas être dans le futur",
            ));
        }
        let holding = input
            .holding_id
            .map(|id| {
                holding_of(portfolio, account, HoldingId::from_uuid(id))
                    .ok_or(AppError::NotFound("Position introuvable dans ce compte"))
            })
            .transpose()?;
        let movement = Movement::new(
            kind,
            MovementFields {
                holding: holding.map(|holding| holding.id),
                quantity: input.quantity,
                unit_price: input.unit_price,
                amount: input.amount,
                fees: input.fees,
            },
        )?;
        Ok(Operation {
            id: existing.map_or_else(OperationId::generate, |op| op.id),
            account_id: account.id,
            date: input.date,
            movement,
            currency: holding.map_or_else(|| account.currency.clone(), |h| h.currency.clone()),
            note: clean(input.note.clone()),
            recorded_at: existing.map_or_else(|| self.clock.now(), |op| op.recorded_at),
        })
    }

    /// Rejoue le journal modifié puis l'enregistre avec les positions recalculées.
    async fn commit(
        &self,
        journal: &[Operation],
        change: OperationChange,
        touched: &[&Operation],
    ) -> AppResult<Ledger> {
        let ledger = Ledger::replay(journal)?;
        let positions: Vec<PositionUpdate> = affected(touched)
            .into_iter()
            .map(|holding| {
                let position = ledger.position(holding);
                PositionUpdate {
                    holding,
                    quantity: position.quantity,
                    buy_price: position.average_price,
                }
            })
            .collect();
        self.operations
            .apply_operation(&change, &positions, self.clock.now())
            .await?;
        Ok(ledger)
    }

    async fn find(&self, id: Uuid) -> AppResult<Operation> {
        self.operations
            .find_operation(OperationId::from_uuid(id))
            .await?
            .ok_or(AppError::NotFound("Opération introuvable"))
    }

    /// Journal du compte, du plus récent au plus ancien.
    pub async fn list(&self, account_id: Uuid) -> AppResult<Vec<OperationView>> {
        let portfolio = self.reader.load().await?;
        let account = positions_account(&portfolio, account_id)?;
        let journal = self.operations.operations(account.id).await?;
        let ledger = Ledger::replay(&journal)?;
        let mut views: Vec<OperationView> = chronological(&journal)
            .into_iter()
            .map(|operation| view(&portfolio, &ledger, operation.clone()))
            .collect();
        views.reverse();
        Ok(views)
    }

    /// Plus-values réalisées, dividendes, frais, versements et retraits par année, en EUR.
    pub async fn summary(&self, account_id: Uuid) -> AppResult<Vec<YearSummary>> {
        let portfolio = self.reader.load().await?;
        let account = positions_account(&portfolio, account_id)?;
        let journal = self.operations.operations(account.id).await?;
        Ok(Ledger::replay(&journal)?.yearly(&journal, &portfolio.rates))
    }

    pub async fn create(
        &self,
        account_id: Uuid,
        input: &OperationInput,
    ) -> AppResult<OperationView> {
        let portfolio = self.reader.load().await?;
        let account = positions_account(&portfolio, account_id)?;
        let operation = self.build(&portfolio, account, input, None)?;
        let mut journal = self.operations.operations(account.id).await?;
        journal.push(operation.clone());
        let ledger = self
            .commit(
                &journal,
                OperationChange::Save(operation.clone()),
                &[&operation],
            )
            .await?;
        Ok(view(&portfolio, &ledger, operation))
    }

    /// Remplace l'opération (même compte, même ordre de saisie).
    pub async fn update(&self, id: Uuid, input: &OperationInput) -> AppResult<OperationView> {
        let existing = self.find(id).await?;
        let portfolio = self.reader.load().await?;
        let account = positions_account(&portfolio, existing.account_id.as_uuid())?;
        let operation = self.build(&portfolio, account, input, Some(&existing))?;
        let mut journal = self.operations.operations(account.id).await?;
        journal.retain(|op| op.id != existing.id);
        journal.push(operation.clone());
        let ledger = self
            .commit(
                &journal,
                OperationChange::Save(operation.clone()),
                &[&existing, &operation],
            )
            .await?;
        Ok(view(&portfolio, &ledger, operation))
    }

    /// Supprime l'opération ; refusé si une vente ultérieure dépasserait alors la quantité détenue.
    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let existing = self.find(id).await?;
        let mut journal = self.operations.operations(existing.account_id).await?;
        journal.retain(|op| op.id != existing.id);
        let change = OperationChange::Delete {
            id: existing.id,
            account: existing.account_id,
        };
        self.commit(&journal, change, &[&existing]).await?;
        Ok(())
    }

    /// Vrai si la position a au moins un achat ou une vente : sa quantité et son PRU viennent du journal.
    pub(crate) async fn drives(&self, holding: &Holding) -> AppResult<bool> {
        Ok(self
            .operations
            .operations(holding.account_id)
            .await?
            .iter()
            .any(|operation| operation.traded_holding() == Some(holding.id)))
    }
}

#[cfg(test)]
mod tests {
    use fineme_domain::DomainError;
    use fineme_domain::holding::PriceSource;
    use fineme_domain::ids::AccountId;

    use super::*;
    use crate::commands::HoldingInput;
    use crate::testing::{TestApp, date, holding_input};

    fn op(kind: &str, day: u32, holding: Option<HoldingId>) -> OperationInput {
        OperationInput {
            kind: kind.into(),
            date: date(2025, 3, day),
            holding_id: holding.map(|id| id.as_uuid()),
            quantity: None,
            unit_price: None,
            amount: None,
            fees: None,
            note: None,
        }
    }

    fn trade(
        kind: &str,
        day: u32,
        holding: HoldingId,
        quantity: f64,
        price: f64,
        fees: f64,
    ) -> OperationInput {
        OperationInput {
            quantity: Some(quantity),
            unit_price: Some(price),
            fees: Some(fees),
            ..op(kind, day, Some(holding))
        }
    }

    fn cash(kind: &str, day: u32, amount: f64) -> OperationInput {
        OperationInput {
            amount: Some(amount),
            ..op(kind, day, None)
        }
    }

    /// PEA avec une position « ETF Monde » saisie à la main (5 parts à 80 €).
    async fn pea_with_etf(t: &TestApp) -> (AccountId, HoldingId) {
        let pea = t.investment(t.alice, 1000.0).await;
        let etf = t
            .app
            .holdings
            .create(
                pea.as_uuid(),
                &holding_input("ETF Monde", 5.0, Some(80.0), 120.0),
            )
            .await
            .unwrap()
            .holding
            .id;
        (pea, etf)
    }

    async fn holding(t: &TestApp, account: AccountId, id: HoldingId) -> Holding {
        t.app
            .accounts
            .get(account.as_uuid(), None)
            .await
            .unwrap()
            .holdings
            .into_iter()
            .find(|view| view.holding.id == id)
            .unwrap()
            .holding
    }

    #[tokio::test]
    async fn trades_recompute_quantity_and_average_price_of_the_holding() {
        let t = TestApp::household().await;
        let (pea, etf) = pea_with_etf(&t).await;
        let ops = &t.app.operations;
        ops.create(pea.as_uuid(), &trade("buy", 1, etf, 10.0, 100.0, 5.0))
            .await
            .unwrap();
        ops.create(pea.as_uuid(), &trade("buy", 2, etf, 10.0, 130.0, 5.0))
            .await
            .unwrap();
        let sale = ops
            .create(pea.as_uuid(), &trade("sell", 3, etf, 8.0, 150.0, 4.0))
            .await
            .unwrap();
        assert!((sale.realized_gain.unwrap() - 272.0).abs() < 1e-9);
        assert_eq!(sale.holding_name.as_deref(), Some("ETF Monde"));
        let h = holding(&t, pea, etf).await;
        // La saisie manuelle (5 parts à 80 €) est remplacée par le journal ; le cours reste.
        assert_eq!(h.quantity, 12.0);
        assert!((h.buy_price.unwrap() - 115.5).abs() < 1e-9);
        assert_eq!(
            (h.current_price, h.price_source),
            (120.0, PriceSource::Manual)
        );
        // Le solde espèces n'est pas touché par le journal.
        let account = t
            .app
            .accounts
            .get(pea.as_uuid(), None)
            .await
            .unwrap()
            .view
            .account;
        assert_eq!(account.cash_balance, 1000.0);
        assert_eq!(account.updated_at, t.clock.now());
    }

    #[tokio::test]
    async fn holding_without_trades_keeps_its_manual_entry() {
        let t = TestApp::household().await;
        let (pea, etf) = pea_with_etf(&t).await;
        let dividend = OperationInput {
            amount: Some(12.0),
            ..op("dividend", 4, Some(etf))
        };
        t.app
            .operations
            .create(pea.as_uuid(), &dividend)
            .await
            .unwrap();
        let h = holding(&t, pea, etf).await;
        assert_eq!((h.quantity, h.buy_price), (5.0, Some(80.0)));
        // Saisie manuelle toujours possible.
        let patch = HoldingInput {
            quantity: Some(6.0),
            ..Default::default()
        };
        t.app.holdings.update(etf.as_uuid(), &patch).await.unwrap();
    }

    #[tokio::test]
    async fn journal_driven_holding_refuses_manual_quantity_or_price_changes() {
        let t = TestApp::household().await;
        let (pea, etf) = pea_with_etf(&t).await;
        t.app
            .operations
            .create(pea.as_uuid(), &trade("buy", 1, etf, 2.0, 50.0, 0.0))
            .await
            .unwrap();
        let change = HoldingInput {
            quantity: Some(3.0),
            ..Default::default()
        };
        assert!(matches!(
            t.app.holdings.update(etf.as_uuid(), &change).await,
            Err(AppError::Conflict(_))
        ));
        // Renvoyer les valeurs actuelles (formulaire complet) et changer le cours reste permis.
        let same = HoldingInput {
            quantity: Some(2.0),
            buy_price: Some(Some(50.0)),
            current_price: Some(55.0),
            ..Default::default()
        };
        let updated = t.app.holdings.update(etf.as_uuid(), &same).await.unwrap();
        assert_eq!(updated.holding.current_price, 55.0);
    }

    #[tokio::test]
    async fn deleting_or_editing_a_trade_replays_the_journal() {
        let t = TestApp::household().await;
        let (pea, etf) = pea_with_etf(&t).await;
        let ops = &t.app.operations;
        let first = ops
            .create(pea.as_uuid(), &trade("buy", 1, etf, 4.0, 10.0, 0.0))
            .await
            .unwrap();
        ops.create(pea.as_uuid(), &trade("sell", 5, etf, 3.0, 12.0, 0.0))
            .await
            .unwrap();
        // Supprimer l'achat rendrait la vente impossible : refusé, rien ne change.
        let refused = ops.delete(first.operation.id.as_uuid()).await;
        assert!(matches!(
            refused,
            Err(AppError::Domain(DomainError::Oversold { .. }))
        ));
        assert_eq!(holding(&t, pea, etf).await.quantity, 1.0);
        // Corriger l'achat (6 au lieu de 4) recalcule la position.
        let edited = ops
            .update(
                first.operation.id.as_uuid(),
                &trade("buy", 1, etf, 6.0, 10.0, 0.0),
            )
            .await
            .unwrap();
        assert_eq!(edited.operation.recorded_at, first.operation.recorded_at);
        assert_eq!(holding(&t, pea, etf).await.quantity, 3.0);
        // Supprimer toutes les opérations remet la position à zéro.
        for view in ops.list(pea.as_uuid()).await.unwrap() {
            ops.delete(view.operation.id.as_uuid()).await.unwrap();
        }
        let h = holding(&t, pea, etf).await;
        assert_eq!((h.quantity, h.buy_price), (0.0, None));
        assert!(matches!(
            ops.delete(first.operation.id.as_uuid()).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn list_is_most_recent_first_and_summary_is_yearly() {
        let t = TestApp::household().await;
        let (pea, etf) = pea_with_etf(&t).await;
        let ops = &t.app.operations;
        ops.create(pea.as_uuid(), &cash("deposit", 1, 2000.0))
            .await
            .unwrap();
        ops.create(pea.as_uuid(), &trade("buy", 2, etf, 10.0, 100.0, 2.0))
            .await
            .unwrap();
        ops.create(pea.as_uuid(), &trade("sell", 9, etf, 5.0, 120.0, 1.0))
            .await
            .unwrap();
        ops.create(pea.as_uuid(), &cash("fee", 10, 3.0))
            .await
            .unwrap();
        let list = ops.list(pea.as_uuid()).await.unwrap();
        let kinds: Vec<&str> = list
            .iter()
            .map(|v| v.operation.movement.kind().as_str())
            .collect();
        assert_eq!(kinds, vec!["fee", "sell", "buy", "deposit"]);
        let years = ops.summary(pea.as_uuid()).await.unwrap();
        assert_eq!(years.len(), 1);
        assert_eq!(years[0].year, 2025);
        assert!((years[0].realized_gain - 98.0).abs() < 1e-9);
        assert!((years[0].fees - 6.0).abs() < 1e-9);
        assert_eq!(years[0].deposits, 2000.0);
    }

    #[tokio::test]
    async fn inputs_are_validated() {
        let t = TestApp::household().await;
        let (pea, etf) = pea_with_etf(&t).await;
        let other = t.investment(t.bob, 0.0).await;
        let ops = &t.app.operations;
        // Position d'un autre compte.
        assert!(matches!(
            ops.create(other.as_uuid(), &trade("buy", 1, etf, 1.0, 1.0, 0.0))
                .await,
            Err(AppError::NotFound(_))
        ));
        // Vente sans titres détenus.
        assert!(matches!(
            ops.create(pea.as_uuid(), &trade("sell", 1, etf, 1.0, 1.0, 0.0))
                .await,
            Err(AppError::Domain(DomainError::Oversold { .. }))
        ));
        assert!(matches!(
            ops.create(pea.as_uuid(), &cash("transfer", 1, 1.0)).await,
            Err(AppError::Domain(_))
        ));
        let future = OperationInput {
            date: t.clock.today().succ_opt().unwrap(),
            ..cash("deposit", 1, 1.0)
        };
        assert!(matches!(
            ops.create(pea.as_uuid(), &future).await,
            Err(AppError::Invalid(_))
        ));
        let (house, _) = t.house_with_loan().await;
        assert!(matches!(
            ops.list(house.as_uuid()).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            ops.summary(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
        assert!(ops.list(pea.as_uuid()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn currency_follows_the_holding_then_the_account() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await;
        let usd = HoldingInput {
            currency: Some("USD".into()),
            ..holding_input("Action US", 0.0, None, 10.0)
        };
        let id = t
            .app
            .holdings
            .create(pea.as_uuid(), &usd)
            .await
            .unwrap()
            .holding
            .id;
        let buy = t
            .app
            .operations
            .create(pea.as_uuid(), &trade("buy", 1, id, 1.0, 10.0, 0.0))
            .await
            .unwrap();
        assert_eq!(buy.operation.currency.as_str(), "USD");
        let deposit = t
            .app
            .operations
            .create(pea.as_uuid(), &cash("deposit", 1, 5.0))
            .await
            .unwrap();
        assert_eq!(deposit.operation.currency.as_str(), "EUR");
    }
}
