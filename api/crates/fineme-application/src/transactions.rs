use std::sync::Arc;

use fineme_domain::account::AccountSource;
use fineme_domain::error::finite;
use fineme_domain::ids::{AccountId, TransactionId};
use fineme_domain::transaction::Transaction;
use uuid::Uuid;

use crate::commands::{TransactionInput, clean};
use crate::error::{AppError, AppResult};
use crate::ports::{AccountWrite, Clock, PortfolioRepository, TransactionRepository};

const DEFAULT_PAGE: i64 = 50;
const MAX_PAGE: i64 = 1000;

#[derive(Debug, Clone, PartialEq)]
pub struct TransactionPage {
    pub items: Vec<Transaction>,
    pub total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportedTransactions {
    pub inserted: u64,
    pub skipped: u64,
}

pub struct TransactionService {
    pub(crate) portfolio: Arc<dyn PortfolioRepository>,
    pub(crate) transactions: Arc<dyn TransactionRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl TransactionService {
    async fn account(&self, account_id: Uuid) -> AppResult<fineme_domain::account::Account> {
        let id = AccountId::from_uuid(account_id);
        let data = self.portfolio.load().await?;
        data.accounts
            .into_iter()
            .find(|account| account.id == id)
            .ok_or(AppError::NotFound("Compte introuvable"))
    }

    pub async fn list(
        &self,
        account_id: Uuid,
        limit: Option<i64>,
        offset: Option<i64>,
    ) -> AppResult<TransactionPage> {
        let account = self.account(account_id).await?;
        let limit = limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
        let (items, total) = self
            .transactions
            .page(account.id, limit, offset.unwrap_or(0).max(0))
            .await?;
        Ok(TransactionPage { items, total })
    }

    /// Import en masse, dédoublonné sur (date, libellé, montant) ; peut fixer le nouveau solde du compte.
    pub async fn bulk(
        &self,
        account_id: Uuid,
        inputs: Vec<TransactionInput>,
        new_cash_balance: Option<f64>,
    ) -> AppResult<ImportedTransactions> {
        let mut account = self.account(account_id).await?;
        let mut transactions = Vec::with_capacity(inputs.len());
        for input in inputs {
            let label = input.label.trim().to_string();
            if label.is_empty() {
                return Err(AppError::invalid("Le libellé d'une transaction est requis"));
            }
            transactions.push(Transaction {
                id: TransactionId::generate(),
                account_id: account.id,
                date: input.date,
                label,
                amount: finite(input.amount, "amount")?,
                category: clean(input.category),
            });
        }
        if let Some(balance) = new_cash_balance {
            account.cash_balance = finite(balance, "new_cash_balance")?;
        }
        let inserted = self.transactions.insert_new(&transactions).await?;
        account.updated_at = self.clock.now();
        if account.source == AccountSource::Manual {
            account.source = AccountSource::Csv;
        }
        self.portfolio
            .save_accounts(&[AccountWrite::account_only(account)])
            .await?;
        Ok(ImportedTransactions {
            inserted,
            skipped: transactions.len() as u64 - inserted,
        })
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        if self
            .transactions
            .delete(TransactionId::from_uuid(id))
            .await?
        {
            Ok(())
        } else {
            Err(AppError::NotFound("Transaction introuvable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestApp, date};

    fn tx(day: u32, label: &str, amount: f64) -> TransactionInput {
        TransactionInput {
            date: date(2026, 1, day),
            label: label.into(),
            amount,
            category: Some("  ".into()),
        }
    }

    #[tokio::test]
    async fn bulk_deduplicates_on_date_label_amount() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 100.0).await.as_uuid();
        let first = vec![
            tx(1, "Courses", -54.2),
            tx(2, "Salaire", 2500.0),
            tx(1, "Courses", -54.2),
        ];
        let result = t
            .app
            .transactions
            .bulk(id, first.clone(), None)
            .await
            .unwrap();
        assert_eq!(
            result,
            ImportedTransactions {
                inserted: 2,
                skipped: 1
            }
        );
        let again = [first, vec![tx(3, "Loyer", -900.0)]].concat();
        assert_eq!(
            t.app.transactions.bulk(id, again, None).await.unwrap(),
            ImportedTransactions {
                inserted: 1,
                skipped: 3
            }
        );
        // Même libellé et même date mais montant différent : ce n'est pas un doublon.
        let other = t
            .app
            .transactions
            .bulk(id, vec![tx(1, "Courses", -10.0)], None)
            .await
            .unwrap();
        assert_eq!(other.inserted, 1);
    }

    #[tokio::test]
    async fn bulk_sets_balance_marks_csv_and_cleans_category() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 100.0).await.as_uuid();
        t.clock.advance_days(2);
        t.app
            .transactions
            .bulk(id, vec![tx(1, " Courses ", -5.0)], Some(3500.0))
            .await
            .unwrap();
        let account = t.app.accounts.get(id, None).await.unwrap().view.account;
        assert_eq!(account.cash_balance, 3500.0);
        assert_eq!(account.source, AccountSource::Csv);
        assert_eq!(account.updated_at, t.clock.now());
        let page = t.app.transactions.list(id, None, None).await.unwrap();
        assert_eq!(
            (page.items[0].label.as_str(), page.items[0].category.clone()),
            ("Courses", None)
        );
    }

    #[tokio::test]
    async fn bulk_validates_and_inserts_nothing_on_error() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 100.0).await.as_uuid();
        let batch = vec![tx(1, "Ok", 1.0), tx(2, "  ", 1.0)];
        assert!(matches!(
            t.app.transactions.bulk(id, batch, Some(1.0)).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app
                .transactions
                .bulk(id, vec![tx(1, "x", f64::NAN)], None)
                .await,
            Err(AppError::Domain(_))
        ));
        assert_eq!(
            t.app.transactions.list(id, None, None).await.unwrap().total,
            0
        );
        assert_eq!(
            t.app
                .accounts
                .get(id, None)
                .await
                .unwrap()
                .view
                .account
                .cash_balance,
            100.0
        );
        assert!(matches!(
            t.app.transactions.bulk(Uuid::new_v4(), vec![], None).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn list_is_paged_most_recent_first() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 0.0).await.as_uuid();
        let batch = (1..=5)
            .map(|day| tx(day, "Mouvement", f64::from(day)))
            .collect();
        t.app.transactions.bulk(id, batch, None).await.unwrap();
        let page = t.app.transactions.list(id, Some(2), Some(1)).await.unwrap();
        assert_eq!(page.total, 5);
        assert_eq!(
            page.items.iter().map(|x| x.amount).collect::<Vec<_>>(),
            vec![4.0, 3.0]
        );
        // Bornes : limite ramenée dans [1, 1000], décalage négatif ignoré.
        assert_eq!(
            t.app
                .transactions
                .list(id, Some(0), Some(-3))
                .await
                .unwrap()
                .items
                .len(),
            1
        );
        assert!(matches!(
            t.app.transactions.list(Uuid::new_v4(), None, None).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn delete_transaction() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 0.0).await.as_uuid();
        t.app
            .transactions
            .bulk(id, vec![tx(1, "A", 1.0)], None)
            .await
            .unwrap();
        let tx_id = t.app.transactions.list(id, None, None).await.unwrap().items[0]
            .id
            .as_uuid();
        t.app.transactions.delete(tx_id).await.unwrap();
        assert!(matches!(
            t.app.transactions.delete(tx_id).await,
            Err(AppError::NotFound(_))
        ));
    }
}
