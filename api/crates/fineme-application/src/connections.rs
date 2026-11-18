//! Connexions bancaires (agrégateur). L'architecture est prête mais aucun fournisseur n'est fourni :
//! sans fournisseur configuré, la liste est vide et la création répond « non configuré ».

use std::sync::Arc;

use fineme_domain::account::{Account, AccountKind, AccountSource};
use fineme_domain::connection::{Connection, ConnectionStatus};
use fineme_domain::fx::Currency;
use fineme_domain::ids::{AccountId, ConnectionId, TransactionId};
use fineme_domain::ownership::Ownership;
use fineme_domain::transaction::Transaction;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{
    BankProvider, Clock, ConnectionRepository, PortfolioRepository, TransactionRepository,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub configured: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StartedConnection {
    pub id: ConnectionId,
    pub redirect_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncReport {
    pub accounts_updated: u64,
    pub transactions_inserted: u64,
}

pub struct ConnectionService {
    pub(crate) connections: Arc<dyn ConnectionRepository>,
    pub(crate) portfolio: Arc<dyn PortfolioRepository>,
    pub(crate) transactions: Arc<dyn TransactionRepository>,
    pub(crate) providers: Vec<Arc<dyn BankProvider>>,
    pub(crate) clock: Arc<dyn Clock>,
}

const NOT_CONFIGURED: AppError =
    AppError::NotConfigured("Ce fournisseur bancaire n'est pas configuré");
const NOT_FOUND: AppError = AppError::NotFound("Connexion introuvable");

impl ConnectionService {
    fn provider(&self, id: &str) -> AppResult<&Arc<dyn BankProvider>> {
        self.providers
            .iter()
            .find(|provider| provider.id() == id && provider.configured())
            .ok_or(NOT_CONFIGURED)
    }

    pub fn providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|p| ProviderInfo {
                id: p.id().to_string(),
                name: p.name().to_string(),
                configured: p.configured(),
            })
            .collect()
    }

    pub async fn list(&self) -> AppResult<Vec<Connection>> {
        Ok(self.connections.list().await?)
    }

    pub async fn create(&self, provider_id: &str) -> AppResult<StartedConnection> {
        let provider = self.provider(provider_id)?;
        let started = provider.create_connection().await?;
        let connection = Connection {
            id: ConnectionId::generate(),
            provider: provider.id().to_string(),
            institution: started.institution,
            status: ConnectionStatus::Active,
            last_sync_at: None,
            error: None,
            external_id: Some(started.external_id),
        };
        self.connections.save(&connection).await?;
        Ok(StartedConnection {
            id: connection.id,
            redirect_url: started.redirect_url,
        })
    }

    /// Récupère comptes et mouvements chez le fournisseur. Un nouveau compte est attribué à 100 %
    /// au membre principal ; un compte déjà connu voit son solde mis à jour.
    pub async fn sync(&self, id: Uuid) -> AppResult<SyncReport> {
        let mut connection = self
            .connections
            .find(ConnectionId::from_uuid(id))
            .await?
            .ok_or(NOT_FOUND)?;
        let provider = self.provider(&connection.provider)?;
        let now = self.clock.now();
        let fetched = match provider
            .fetch_accounts(connection.external_id.as_deref().unwrap_or_default())
            .await
        {
            Ok(accounts) => accounts,
            Err(error) => {
                connection.status = ConnectionStatus::Error;
                connection.error = Some(error.to_string());
                self.connections.save(&connection).await?;
                return Err(error.into());
            }
        };
        let data = self.portfolio.load().await?;
        let primary = data
            .members
            .iter()
            .find(|member| member.is_primary)
            .or(data.members.first())
            .ok_or(AppError::Conflict(
                "Aucun membre du foyer : terminez d'abord la configuration",
            ))?;

        let mut report = SyncReport {
            accounts_updated: 0,
            transactions_inserted: 0,
        };
        for bank_account in fetched {
            let external_id = format!("{}:{}", connection.provider, bank_account.external_id);
            let account = match data
                .accounts
                .iter()
                .find(|a| a.external_id.as_deref() == Some(external_id.as_str()))
            {
                Some(existing) => Account {
                    cash_balance: bank_account.balance,
                    connection_id: Some(connection.id),
                    updated_at: now,
                    ..existing.clone()
                },
                None => Account {
                    id: AccountId::generate(),
                    name: bank_account.name,
                    institution: bank_account.institution,
                    kind: AccountKind::parse(&bank_account.kind)?,
                    envelope: None,
                    currency: Currency::parse(&bank_account.currency)?,
                    cash_balance: bank_account.balance,
                    annual_yield_pct: None,
                    source: AccountSource::Aggregator,
                    external_id: Some(external_id),
                    connection_id: Some(connection.id),
                    opened_at: None,
                    updated_at: now,
                    owners: Ownership::sole(primary.id),
                    property: None,
                    loan: None,
                },
            };
            let transactions: Vec<Transaction> = bank_account
                .transactions
                .into_iter()
                .map(|t| Transaction {
                    id: TransactionId::generate(),
                    account_id: account.id,
                    date: t.date,
                    label: t.label,
                    amount: t.amount,
                    category: t.category,
                })
                .collect();
            report.transactions_inserted +=
                self.transactions.import(&account, &transactions).await?;
            report.accounts_updated += 1;
        }
        connection.status = ConnectionStatus::Active;
        connection.error = None;
        connection.last_sync_at = Some(now);
        self.connections.save(&connection).await?;
        Ok(report)
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        if self.connections.delete(ConnectionId::from_uuid(id)).await? {
            Ok(())
        } else {
            Err(NOT_FOUND)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{BankAccount, BankTransaction};
    use crate::testing::{FakeBank, TestApp, date};

    fn bank_account(balance: f64) -> BankAccount {
        BankAccount {
            external_id: "acc-1".into(),
            name: "Compte Démo".into(),
            institution: Some("Banque Démo".into()),
            kind: "checking".into(),
            currency: "EUR".into(),
            balance,
            transactions: vec![
                BankTransaction {
                    date: date(2026, 1, 2),
                    label: "Café".into(),
                    amount: -2.5,
                    category: None,
                },
                BankTransaction {
                    date: date(2026, 1, 3),
                    label: "Virement".into(),
                    amount: 100.0,
                    category: None,
                },
            ],
        }
    }

    #[tokio::test]
    async fn without_provider_nothing_is_available() {
        let t = TestApp::household().await;
        assert!(t.app.connections.providers().is_empty());
        assert!(t.app.connections.list().await.unwrap().is_empty());
        assert!(matches!(
            t.app.connections.create("demo").await,
            Err(AppError::NotConfigured(_))
        ));
        assert!(matches!(
            t.app.connections.sync(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app.connections.delete(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn unconfigured_provider_is_listed_but_unusable() {
        let t = TestApp::with_bank(FakeBank::new(false, vec![])).await;
        assert_eq!(
            t.app.connections.providers(),
            vec![ProviderInfo {
                id: "demo".into(),
                name: "Banque Démo".into(),
                configured: false
            }]
        );
        assert!(matches!(
            t.app.connections.create("demo").await,
            Err(AppError::NotConfigured(_))
        ));
    }

    #[tokio::test]
    async fn create_then_sync_imports_accounts_and_transactions_once() {
        let t = TestApp::with_bank(FakeBank::new(true, vec![bank_account(250.0)])).await;
        let started = t.app.connections.create("demo").await.unwrap();
        assert_eq!(started.redirect_url, "https://banque.example/autoriser");
        let report = t.app.connections.sync(started.id.as_uuid()).await.unwrap();
        assert_eq!(
            report,
            SyncReport {
                accounts_updated: 1,
                transactions_inserted: 2
            }
        );
        let accounts = t.app.accounts.list(None).await.unwrap();
        assert_eq!(accounts.len(), 1);
        let account = &accounts[0].account;
        assert_eq!(account.source, AccountSource::Aggregator);
        assert_eq!(account.external_id.as_deref(), Some("demo:acc-1"));
        assert_eq!(account.connection_id, Some(started.id));
        assert_eq!(accounts[0].owners[0].name, "Alice");
        assert_eq!(accounts[0].owned_value, 250.0);

        // Deuxième synchronisation : même compte, aucun doublon de mouvement.
        let again = t.app.connections.sync(started.id.as_uuid()).await.unwrap();
        assert_eq!(
            again,
            SyncReport {
                accounts_updated: 1,
                transactions_inserted: 0
            }
        );
        assert_eq!(t.app.accounts.list(None).await.unwrap().len(), 1);
        let connection = &t.app.connections.list().await.unwrap()[0];
        assert_eq!(connection.status, ConnectionStatus::Active);
        assert_eq!(connection.last_sync_at, Some(t.clock.now()));
    }

    #[tokio::test]
    async fn provider_failure_marks_the_connection_in_error() {
        let bank = FakeBank::new(true, vec![]);
        let t = TestApp::with_bank(bank.clone()).await;
        let started = t.app.connections.create("demo").await.unwrap();
        bank.fail_next_fetch("jeton expiré");
        assert!(matches!(
            t.app.connections.sync(started.id.as_uuid()).await,
            Err(AppError::Provider(_))
        ));
        let connection = &t.app.connections.list().await.unwrap()[0];
        assert_eq!(connection.status, ConnectionStatus::Error);
        assert_eq!(connection.error.as_deref(), Some("jeton expiré"));
        t.app
            .connections
            .delete(started.id.as_uuid())
            .await
            .unwrap();
        assert!(t.app.connections.list().await.unwrap().is_empty());
    }
}
