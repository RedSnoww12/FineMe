//! Budget : vue mensuelle (revenus, dépenses, épargne), transactions catégorisées, règles et cibles.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use chrono::{Duration, NaiveDate};
use fineme_domain::account::Account;
use fineme_domain::budget::{
    BudgetLine, BudgetTarget, Categorizer, CategoryRule, CategorySource, Flows, Month,
    RecurringCharge, TRANSFER_MAX_DAYS, default_categories, detect_internal_transfers,
    detect_recurring, expenses_by_category, flows, is_internal, trend,
};
use fineme_domain::ids::{AccountId, BudgetRuleId};
use fineme_domain::ownership::View;
use fineme_domain::portfolio::Portfolio;
use fineme_domain::transaction::Transaction;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{BudgetRepository, TransactionRepository};
use crate::reader::{PortfolioReader, view_for};

/// Nombre de mois de l'évolution (mois demandé inclus), aussi fenêtre d'analyse des charges récurrentes.
const TREND_MONTHS: u32 = 12;

#[derive(Debug, Clone, PartialEq)]
pub struct CategorySpending {
    pub category: Option<String>,
    pub amount: f64,
    pub target: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetMonth {
    pub flows: Flows,
    pub categories: Vec<CategorySpending>,
    pub trend: Vec<Flows>,
    pub recurring: Vec<RecurringCharge>,
    pub internal_transfers: usize,
}

/// Mouvement vu par le budget, avec sa catégorie effective et son statut de virement interne.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetTransaction {
    pub transaction: Transaction,
    pub account_name: String,
    /// Signé, EUR × part de la vue.
    pub amount_eur: f64,
    pub category: Option<(String, CategorySource)>,
    pub internal_transfer: bool,
}

pub struct BudgetService {
    pub(crate) reader: PortfolioReader,
    pub(crate) transactions: Arc<dyn TransactionRepository>,
    pub(crate) budget: Arc<dyn BudgetRepository>,
}

impl BudgetService {
    pub async fn month(
        &self,
        month: Option<&str>,
        member_id: Option<Uuid>,
    ) -> AppResult<BudgetMonth> {
        let (portfolio, view, month) = self.context(month, member_id).await?;
        let first = month.back(TREND_MONTHS - 1);
        let classified = self
            .classify(&portfolio, view, first.first_day(), month.last_day())
            .await?;
        let lines: Vec<BudgetLine> = classified.iter().map(line).collect();
        let targets: HashMap<String, f64> = self
            .budget
            .targets()
            .await?
            .into_iter()
            .map(|target| (target.category().to_string(), target.monthly_amount()))
            .collect();
        let mut categories: Vec<CategorySpending> = expenses_by_category(&lines, month)
            .into_iter()
            .map(|(category, amount)| CategorySpending {
                target: category.as_ref().and_then(|c| targets.get(c).copied()),
                category,
                amount,
            })
            .collect();
        let mut unspent: Vec<_> = targets
            .iter()
            .filter(|(name, _)| {
                !categories
                    .iter()
                    .any(|c| c.category.as_deref() == Some(name.as_str()))
            })
            .map(|(name, amount)| CategorySpending {
                category: Some(name.clone()),
                amount: 0.0,
                target: Some(*amount),
            })
            .collect();
        unspent.sort_by(|a, b| a.category.cmp(&b.category));
        categories.extend(unspent);
        Ok(BudgetMonth {
            flows: flows(&lines, month),
            categories,
            trend: trend(&lines, month, TREND_MONTHS),
            recurring: detect_recurring(&lines),
            internal_transfers: classified
                .iter()
                .filter(|c| c.internal_transfer && month.contains(c.transaction.date))
                .count(),
        })
    }

    /// Mouvements du mois pour la vue, du plus récent au plus ancien.
    pub async fn transactions(
        &self,
        month: Option<&str>,
        member_id: Option<Uuid>,
    ) -> AppResult<Vec<BudgetTransaction>> {
        let (portfolio, view, month) = self.context(month, member_id).await?;
        let mut items = self
            .classify(&portfolio, view, month.first_day(), month.last_day())
            .await?;
        items.sort_by(|a, b| {
            b.transaction
                .date
                .cmp(&a.transaction.date)
                .then_with(|| a.transaction.label.cmp(&b.transaction.label))
        });
        Ok(items)
    }

    /// Catégories connues : par défaut, des règles, des cibles et des mouvements.
    pub async fn categories(&self) -> AppResult<Vec<String>> {
        let mut all: BTreeSet<String> = default_categories();
        all.extend(
            self.budget
                .rules()
                .await?
                .iter()
                .map(|r| r.category().to_string()),
        );
        all.extend(
            self.budget
                .targets()
                .await?
                .iter()
                .map(|t| t.category().to_string()),
        );
        all.extend(self.transactions.categories().await?);
        Ok(all.into_iter().collect())
    }

    pub async fn rules(&self) -> AppResult<Vec<CategoryRule>> {
        let mut rules = self.budget.rules().await?;
        rules.sort_by(|a, b| a.pattern().cmp(b.pattern()));
        Ok(rules)
    }

    pub async fn delete_rule(&self, id: Uuid) -> AppResult<()> {
        if self.budget.delete_rule(BudgetRuleId::from_uuid(id)).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("Règle introuvable"))
        }
    }

    pub async fn targets(&self) -> AppResult<Vec<BudgetTarget>> {
        let mut targets = self.budget.targets().await?;
        targets.sort_by(|a, b| a.category().cmp(b.category()));
        Ok(targets)
    }

    /// Crée, remplace ou (montant absent) supprime la cible mensuelle d'une catégorie.
    pub async fn set_target(
        &self,
        category: &str,
        monthly_amount: Option<f64>,
    ) -> AppResult<Vec<BudgetTarget>> {
        match monthly_amount {
            Some(amount) => {
                self.budget
                    .save_target(&BudgetTarget::new(category, amount)?)
                    .await?
            }
            None => {
                self.budget.delete_target(category.trim()).await?;
            }
        }
        self.targets().await
    }

    async fn context(
        &self,
        month: Option<&str>,
        member_id: Option<Uuid>,
    ) -> AppResult<(Portfolio, View, Month)> {
        let portfolio = self.reader.load().await?;
        let view = view_for(&portfolio, member_id)?;
        let month = match month {
            Some(text) => Month::parse(text)?,
            None => Month::of(portfolio.today),
        };
        Ok((portfolio, view, month))
    }

    /// Mouvements de la période pour la vue, catégorisés et marqués. Les virements internes sont détectés sur
    /// tous les comptes du foyer, avec une marge de quelques jours autour de la période.
    async fn classify(
        &self,
        portfolio: &Portfolio,
        view: View,
        from: NaiveDate,
        to: NaiveDate,
    ) -> AppResult<Vec<BudgetTransaction>> {
        let margin = Duration::days(TRANSFER_MAX_DAYS);
        let all = self
            .transactions
            .between(from - margin, to + margin)
            .await?;
        let detected = detect_internal_transfers(&all);
        let categorizer = Categorizer::new(self.budget.rules().await?);
        let accounts: HashMap<AccountId, &Account> = portfolio
            .accounts
            .iter()
            .map(|account| (account.id, account))
            .collect();
        Ok(all
            .into_iter()
            .filter(|t| t.date >= from && t.date <= to)
            .filter_map(|transaction| {
                let account = accounts.get(&transaction.account_id)?;
                let fraction = account.owners.fraction(view);
                (fraction > 0.0).then(|| BudgetTransaction {
                    account_name: account.name.clone(),
                    amount_eur: portfolio
                        .rates
                        .to_eur(transaction.amount, &account.currency)
                        * fraction,
                    category: categorizer.categorize(&transaction),
                    internal_transfer: is_internal(&transaction, &detected),
                    transaction,
                })
            })
            .collect())
    }
}

fn line(item: &BudgetTransaction) -> BudgetLine {
    BudgetLine {
        account_id: item.transaction.account_id,
        date: item.transaction.date,
        label: item.transaction.label.clone(),
        amount: item.amount_eur,
        category: item.category.as_ref().map(|(name, _)| name.clone()),
        internal: item.internal_transfer,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{AccountInput, TransactionInput, TransactionPatch};
    use crate::testing::{TestApp, date, owners};

    fn tx(year: i32, month: u32, day: u32, label: &str, amount: f64) -> TransactionInput {
        TransactionInput {
            date: date(year, month, day),
            label: label.into(),
            amount,
            category: None,
        }
    }

    async fn savings(t: &TestApp) -> Uuid {
        t.create(AccountInput {
            name: Some("Livret".into()),
            kind: Some("savings".into()),
            owners: Some(owners(&[(t.alice, 1.0)])),
            ..Default::default()
        })
        .await
        .as_uuid()
    }

    #[tokio::test]
    async fn month_view_excludes_transfers_and_applies_shares() {
        let t = TestApp::household().await;
        let joint = t
            .checking(&[(t.alice, 0.5), (t.bob, 0.5)], 0.0)
            .await
            .as_uuid();
        let livret = savings(&t).await;
        let batch = vec![
            tx(2026, 1, 1, "VIR SALAIRE ACME", 4000.0),
            tx(2026, 1, 5, "PRLV LOYER", -1200.0),
            tx(2026, 1, 9, "CB CARREFOUR 0901", -300.0),
            tx(2026, 1, 31, "VIR VERS LIVRET", -1000.0),
        ];
        t.app.transactions.bulk(joint, batch, None).await.unwrap();
        // Jambe d'arrivée le mois suivant, dans la marge de 3 jours.
        t.app
            .transactions
            .bulk(
                livret,
                vec![tx(2026, 2, 2, "VIR DEPUIS COMPTE", 1000.0)],
                None,
            )
            .await
            .unwrap();
        t.app
            .budget
            .set_target("Alimentation", Some(250.0))
            .await
            .unwrap();
        t.app
            .budget
            .set_target("Loisirs", Some(100.0))
            .await
            .unwrap();

        let household = t.app.budget.month(Some("2026-01"), None).await.unwrap();
        assert_eq!(
            (
                household.flows.income,
                household.flows.expenses,
                household.flows.savings
            ),
            (4000.0, 1500.0, 2500.0)
        );
        assert_eq!(household.flows.savings_rate, Some(0.625));
        assert_eq!(household.internal_transfers, 1);
        assert_eq!(household.trend.len(), 12);
        assert_eq!(household.trend[11].month.to_string(), "2026-01");
        assert_eq!(household.trend[0].month.to_string(), "2025-02");
        let summary: Vec<_> = household
            .categories
            .iter()
            .map(|c| (c.category.as_deref(), c.amount, c.target))
            .collect();
        assert_eq!(
            summary,
            vec![
                (Some("Logement"), 1200.0, None),
                (Some("Alimentation"), 300.0, Some(250.0)),
                (Some("Loisirs"), 0.0, Some(100.0)),
            ]
        );

        // Bob : la moitié du compte joint, rien du livret.
        let bob = t
            .app
            .budget
            .month(Some("2026-01"), Some(t.bob.as_uuid()))
            .await
            .unwrap();
        assert_eq!((bob.flows.income, bob.flows.expenses), (2000.0, 750.0));
        let february = t
            .app
            .budget
            .transactions(Some("2026-02"), Some(t.bob.as_uuid()))
            .await
            .unwrap();
        assert!(february.is_empty());
        let mine = t
            .app
            .budget
            .transactions(Some("2026-02"), None)
            .await
            .unwrap();
        assert!(mine[0].internal_transfer);
    }

    #[tokio::test]
    async fn manual_recategorization_applies_to_same_label() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 0.0).await.as_uuid();
        let batch = vec![
            tx(2026, 1, 3, "CB LIBRAIRIE DU COIN 03/01", -20.0),
            tx(2026, 1, 17, "CB LIBRAIRIE DU COIN 17/01", -15.0),
        ];
        t.app.transactions.bulk(id, batch, None).await.unwrap();
        let items = t
            .app
            .budget
            .transactions(Some("2026-01"), None)
            .await
            .unwrap();
        assert!(items.iter().all(|item| item.category.is_none()));
        t.app
            .transactions
            .update(
                items[1].transaction.id.as_uuid(),
                TransactionPatch {
                    category: Some(Some("Culture".into())),
                    internal_transfer: None,
                },
            )
            .await
            .unwrap();
        // Transaction suivante, importée après la recatégorisation.
        t.app
            .transactions
            .bulk(
                id,
                vec![tx(2026, 1, 28, "CB LIBRAIRIE DU COIN 28/01", -9.0)],
                None,
            )
            .await
            .unwrap();
        let items = t
            .app
            .budget
            .transactions(Some("2026-01"), None)
            .await
            .unwrap();
        let sources: Vec<_> = items
            .iter()
            .map(|item| item.category.clone().map(|(c, s)| (c, s.as_str())))
            .collect();
        assert_eq!(
            sources,
            vec![
                Some(("Culture".into(), "rule")),
                Some(("Culture".into(), "rule")),
                Some(("Culture".into(), "manual")),
            ]
        );
        assert!(
            t.app
                .budget
                .categories()
                .await
                .unwrap()
                .contains(&"Culture".to_string())
        );
        let rules = t.app.budget.rules().await.unwrap();
        assert_eq!(rules[0].pattern(), "cb librairie du coin");
        t.app
            .budget
            .delete_rule(rules[0].id.as_uuid())
            .await
            .unwrap();
        assert!(matches!(
            t.app.budget.delete_rule(rules[0].id.as_uuid()).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn targets_and_validation() {
        let t = TestApp::household().await;
        assert_eq!(
            t.app
                .budget
                .set_target("Loisirs", Some(80.0))
                .await
                .unwrap()
                .len(),
            1
        );
        let targets = t
            .app
            .budget
            .set_target("Loisirs", Some(120.0))
            .await
            .unwrap();
        assert_eq!(targets[0].monthly_amount(), 120.0);
        assert!(
            t.app
                .budget
                .set_target(" Loisirs ", None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(matches!(
            t.app.budget.set_target("Loisirs", Some(-5.0)).await,
            Err(AppError::Domain(_))
        ));
        assert!(matches!(
            t.app.budget.month(Some("2026/01"), None).await,
            Err(AppError::Domain(_))
        ));
        assert!(matches!(
            t.app.budget.month(None, Some(Uuid::new_v4())).await,
            Err(AppError::NotFound(_))
        ));
        // Mois courant par défaut, sans transaction.
        let empty = t.app.budget.month(None, None).await.unwrap();
        assert_eq!(empty.flows.month.to_string(), "2026-01");
        assert_eq!(empty.flows.savings_rate, None);
    }
}
