//! Import / export complet du patrimoine (`Bundle` version 1).

use std::sync::Arc;

use fineme_domain::account::{Account, AccountKind, AccountSource};
use fineme_domain::goal::Goal;
use fineme_domain::holding::Holding;
use fineme_domain::ids::MemberId;
use fineme_domain::member::{Member, default_color};
use fineme_domain::portfolio::Portfolio;

use crate::accounts::{Origin, build_account};
use crate::commands::{
    AccountInput, GoalInput, HoldingInput, LoanInput, OwnerInput, PropertyInput, clean,
};
use crate::error::{AppError, AppResult};
use crate::goals::build_goal;
use crate::holdings::build_holding;
use crate::ports::{AccountWrite, Clock, GoalRepository, ImportBatch, ImportRepository};
use crate::reader::PortfolioReader;

pub const BUNDLE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct BundleMember {
    pub name: String,
    pub color: Option<String>,
    pub is_primary: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleOwner {
    pub member: String,
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleAccount {
    pub external_id: String,
    /// Champs du compte ; `owners` y est ignoré au profit de la liste par nom ci-dessous.
    pub account: AccountInput,
    pub owners: Vec<BundleOwner>,
    pub holdings: Option<Vec<HoldingInput>>,
    /// Pour un prêt : compte immobilier financé, désigné par son `external_id`.
    pub property_external_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BundleGoal {
    pub goal: GoalInput,
    pub account_external_ids: Vec<String>,
    pub members: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bundle {
    pub version: u32,
    pub members: Vec<BundleMember>,
    pub accounts: Vec<BundleAccount>,
    pub goals: Vec<BundleGoal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportCounts {
    pub members: usize,
    pub accounts: usize,
    pub holdings: usize,
    pub goals: usize,
}

pub struct BundleService {
    pub(crate) reader: PortfolioReader,
    pub(crate) goals: Arc<dyn GoalRepository>,
    pub(crate) imports: Arc<dyn ImportRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

/// Identifiant externe d'un compte : le sien, à défaut son identifiant interne (rend l'aller-retour idempotent).
fn external_id(account: &Account) -> String {
    account
        .external_id
        .clone()
        .unwrap_or_else(|| account.id.to_string())
}

fn matches_external(account: &Account, external: &str) -> bool {
    account.external_id.as_deref() == Some(external) || account.id.to_string() == external
}

fn find_account<'a>(accounts: &'a [Account], external: &str) -> AppResult<&'a Account> {
    accounts
        .iter()
        .find(|account| matches_external(account, external.trim()))
        .ok_or_else(|| AppError::invalid(format!("Compte inconnu dans le bundle : {external}")))
}

fn find_member(members: &[Member], name: &str) -> AppResult<MemberId> {
    members
        .iter()
        .find(|member| member.name == name.trim())
        .map(|member| member.id)
        .ok_or_else(|| AppError::invalid(format!("Membre inconnu dans le bundle : {name}")))
}

fn export_account(account: &Account, portfolio: &Portfolio) -> BundleAccount {
    let property = account.property.as_ref().map(|p| PropertyInput {
        address: Some(p.address.clone()),
        category: Some(p.category.as_str().to_string()),
        building_type: Some(p.building_type.as_str().to_string()),
        surface_m2: Some(p.surface_m2),
        purchase_price: Some(p.purchase_price),
        purchase_fees: Some(p.purchase_fees),
        purchased_at: Some(p.purchased_at),
        current_value: Some(p.current_value),
        monthly_rent: Some(p.monthly_rent),
        monthly_charges: Some(p.monthly_charges),
        yearly_property_tax: Some(p.yearly_property_tax),
    });
    let loan = account.loan.as_ref().map(|l| LoanInput {
        property_account_id: None,
        principal: Some(l.terms.principal()),
        interest_rate_pct: Some(l.terms.interest_rate().value()),
        insurance_rate_pct: Some(l.terms.insurance_rate().value()),
        duration_months: i32::try_from(l.terms.duration_months()).ok(),
        start_date: Some(l.terms.start_date()),
    });
    let holdings = account.kind.can_hold_positions().then(|| {
        portfolio
            .holdings_of(account.id)
            .iter()
            .map(|h| HoldingInput {
                kind: Some(h.kind.as_str().to_string()),
                name: Some(h.name.clone()),
                isin: Some(h.isin.clone()),
                symbol: Some(h.symbol.clone()),
                security_type: Some(h.security_type.clone()),
                quantity: Some(h.quantity),
                buy_price: Some(h.buy_price),
                current_price: Some(h.current_price),
                currency: Some(h.currency.as_str().to_string()),
                expense_ratio: Some(h.expense_ratio),
                price_source: Some(h.price_source.as_str().to_string()),
            })
            .collect()
    });
    BundleAccount {
        external_id: external_id(account),
        owners: account
            .owners
            .owners()
            .iter()
            .filter_map(|(id, share)| {
                portfolio.member(*id).map(|member| BundleOwner {
                    member: member.name.clone(),
                    share: share.value(),
                })
            })
            .collect(),
        property_external_id: account
            .loan
            .as_ref()
            .and_then(|loan| loan.property_account_id)
            .and_then(|id| portfolio.account(id))
            .map(external_id),
        holdings,
        account: AccountInput {
            name: Some(account.name.clone()),
            institution: Some(account.institution.clone()),
            kind: Some(account.kind.as_str().to_string()),
            envelope: Some(account.envelope.clone()),
            currency: Some(account.currency.as_str().to_string()),
            cash_balance: Some(account.cash_balance),
            annual_yield_pct: Some(account.annual_yield_pct),
            opened_at: Some(account.opened_at),
            owners: None,
            property,
            loan,
        },
    }
}

fn export_goal(goal: &Goal, portfolio: &Portfolio) -> BundleGoal {
    BundleGoal {
        account_external_ids: goal
            .account_ids
            .iter()
            .filter_map(|id| portfolio.account(*id))
            .map(external_id)
            .collect(),
        members: goal
            .member_ids
            .iter()
            .filter_map(|id| portfolio.member(*id))
            .map(|m| m.name.clone())
            .collect(),
        goal: GoalInput {
            name: Some(goal.name.clone()),
            icon: Some(goal.icon.clone()),
            kind: Some(goal.kind.as_str().to_string()),
            target_amount: Some(goal.target_amount),
            target_date: Some(goal.target_date),
            monthly_contribution: Some(goal.monthly_contribution),
            expected_return_pct: Some(goal.expected_return_pct),
            monthly_expenses: Some(goal.monthly_expenses),
            coverage_months: Some(goal.coverage_months),
            account_ids: None,
            member_ids: None,
        },
    }
}

impl BundleService {
    pub async fn export(&self) -> AppResult<Bundle> {
        let portfolio = self.reader.load().await?;
        let goals = self.goals.list().await?;
        Ok(Bundle {
            version: BUNDLE_VERSION,
            members: portfolio
                .members
                .iter()
                .map(|m| BundleMember {
                    name: m.name.clone(),
                    color: Some(m.color.clone()),
                    is_primary: Some(m.is_primary),
                })
                .collect(),
            accounts: portfolio
                .accounts
                .iter()
                .map(|account| export_account(account, &portfolio))
                .collect(),
            goals: goals
                .iter()
                .map(|goal| export_goal(goal, &portfolio))
                .collect(),
        })
    }

    /// Import idempotent : membres rapprochés par nom, comptes par `external_id` (les positions fournies
    /// remplacent celles du compte), objectifs par nom. Tout le bundle est validé puis appliqué en une fois.
    pub async fn import(&self, bundle: Bundle) -> AppResult<ImportCounts> {
        if bundle.version != BUNDLE_VERSION {
            return Err(AppError::invalid(
                "Version de bundle non prise en charge (attendu : 1)",
            ));
        }
        let mut portfolio = self.reader.load().await?;
        let mut goals = self.goals.list().await?;
        let now = self.clock.now();
        let mut batch = ImportBatch::default();
        let mut counts = ImportCounts {
            members: bundle.members.len(),
            accounts: bundle.accounts.len(),
            holdings: 0,
            goals: bundle.goals.len(),
        };

        for (index, entry) in bundle.members.into_iter().enumerate() {
            let name = entry.name.trim().to_string();
            if name.is_empty() {
                return Err(AppError::invalid("Le nom d'un membre du bundle est vide"));
            }
            let color = clean(entry.color);
            let member = match portfolio
                .members
                .iter_mut()
                .find(|member| member.name == name)
            {
                Some(existing) => {
                    if let Some(color) = color {
                        existing.color = color;
                    }
                    existing.clone()
                }
                None => {
                    let member = Member {
                        id: MemberId::generate(),
                        name,
                        color: color.unwrap_or_else(|| {
                            default_color(portfolio.members.len() + index).to_string()
                        }),
                        // Le rôle de membre principal n'est repris que si personne ne l'occupe.
                        is_primary: entry.is_primary.unwrap_or(false)
                            && !portfolio.members.iter().any(|m| m.is_primary),
                    };
                    portfolio.members.push(member.clone());
                    member
                }
            };
            batch.members.push(member);
        }

        let mut loan_links: Vec<(usize, String)> = Vec::new();
        for entry in bundle.accounts {
            let external = entry.external_id.trim().to_string();
            if external.is_empty() {
                return Err(AppError::invalid(
                    "external_id est requis pour chaque compte du bundle",
                ));
            }
            let owners = entry
                .owners
                .iter()
                .map(|owner| {
                    Ok(OwnerInput {
                        member_id: find_member(&portfolio.members, &owner.member)?.as_uuid(),
                        share: owner.share,
                    })
                })
                .collect::<AppResult<Vec<_>>>()?;
            let input = AccountInput {
                owners: Some(owners),
                ..entry.account
            };
            let existing = portfolio
                .accounts
                .iter()
                .position(|account| matches_external(account, &external));
            let origin = Origin {
                source: AccountSource::Import,
                external_id: Some(external),
            };
            let account = build_account(
                existing.map(|i| &portfolio.accounts[i]),
                &input,
                origin,
                &portfolio.members,
                &portfolio.accounts,
                now,
            )?;
            let holdings = match &entry.holdings {
                Some(inputs) if account.kind.can_hold_positions() => {
                    counts.holdings += inputs.len();
                    let built = inputs
                        .iter()
                        .map(|h| build_holding(None, &account, h, now))
                        .collect::<AppResult<Vec<Holding>>>()?;
                    portfolio.holdings.insert(account.id, built.clone());
                    Some(built)
                }
                _ => None,
            };
            if let Some(property_external_id) = entry
                .property_external_id
                .filter(|_| account.loan.is_some())
            {
                loan_links.push((batch.accounts.len(), property_external_id));
            }
            match existing {
                Some(i) => portfolio.accounts[i] = account.clone(),
                None => portfolio.accounts.push(account.clone()),
            }
            batch.accounts.push(AccountWrite { account, holdings });
        }

        // Liens prêt → bien, une fois tous les comptes connus (l'ordre du bundle n'importe pas).
        for (write_index, property_external_id) in loan_links {
            let property = find_account(&portfolio.accounts, &property_external_id)?;
            if property.kind != AccountKind::RealEstate {
                return Err(AppError::invalid(
                    "property_account_id doit désigner un compte immobilier existant",
                ));
            }
            let property_id = property.id;
            let write = &mut batch.accounts[write_index];
            if let Some(loan) = &mut write.account.loan {
                loan.property_account_id = Some(property_id);
            }
            if let Some(stored) = portfolio
                .accounts
                .iter_mut()
                .find(|account| account.id == write.account.id)
            {
                stored.loan = write.account.loan.clone();
            }
        }

        for entry in bundle.goals {
            let name = entry
                .goal
                .name
                .clone()
                .unwrap_or_default()
                .trim()
                .to_string();
            let account_ids = entry
                .account_external_ids
                .iter()
                .map(|external| {
                    find_account(&portfolio.accounts, external).map(|account| account.id.as_uuid())
                })
                .collect::<AppResult<Vec<_>>>()?;
            let member_ids = entry
                .members
                .iter()
                .map(|member| find_member(&portfolio.members, member).map(|id| id.as_uuid()))
                .collect::<AppResult<Vec<_>>>()?;
            let input = GoalInput {
                account_ids: Some(account_ids),
                member_ids: Some(member_ids),
                ..entry.goal
            };
            let existing = goals.iter().position(|goal| goal.name == name);
            let goal = build_goal(existing.map(|i| &goals[i]), &input, &portfolio)?;
            match existing {
                Some(i) => goals[i] = goal.clone(),
                None => goals.push(goal.clone()),
            }
            batch.goals.push(goal);
        }

        self.imports.apply(&batch).await?;
        Ok(counts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::GoalInput;
    use crate::testing::{TestApp, date, holding_input, loan_input, property_input};
    use fineme_domain::ids::AccountId;

    fn member(name: &str) -> BundleMember {
        BundleMember {
            name: name.into(),
            color: None,
            is_primary: None,
        }
    }

    fn account(external_id: &str, kind: &str, owners: &[(&str, f64)]) -> BundleAccount {
        BundleAccount {
            external_id: external_id.into(),
            account: AccountInput {
                name: Some(format!("Compte {external_id}")),
                kind: Some(kind.into()),
                ..Default::default()
            },
            owners: owners
                .iter()
                .map(|(m, share)| BundleOwner {
                    member: m.to_string(),
                    share: *share,
                })
                .collect(),
            holdings: None,
            property_external_id: None,
        }
    }

    /// Bundle fictif : livret, PEA avec une position, maison et prêt lié (listé avant la maison), un objectif.
    fn sample() -> Bundle {
        let mut savings = account("livret-1", "savings", &[("Alice", 1.0)]);
        savings.account.cash_balance = Some(5000.0);
        let mut pea = account("pea-1", "investment", &[("Alice", 1.0)]);
        pea.holdings = Some(vec![holding_input("ETF Monde", 10.0, Some(20.0), 25.0)]);
        let mut loan = account("pret-1", "loan", &[("Alice", 0.5), ("Bob", 0.5)]);
        loan.account.loan = Some(loan_input(None));
        loan.property_external_id = Some("maison-1".into());
        let mut house = account("maison-1", "real_estate", &[("Alice", 0.5), ("Bob", 0.5)]);
        house.account.property = Some(property_input());
        let goal = BundleGoal {
            goal: GoalInput {
                name: Some("Matelas".into()),
                kind: Some("safety_net".into()),
                monthly_expenses: Some(Some(1000.0)),
                coverage_months: Some(Some(6.0)),
                ..Default::default()
            },
            account_external_ids: vec!["livret-1".into()],
            members: vec!["Alice".into()],
        };
        Bundle {
            version: 1,
            members: vec![
                member("Alice"),
                BundleMember {
                    color: Some("#abcdef".into()),
                    ..member("Bob")
                },
            ],
            accounts: vec![savings, pea, loan, house],
            goals: vec![goal],
        }
    }

    async fn snapshot(t: &TestApp) -> (usize, usize, usize, usize, f64) {
        let state = t.store.state();
        let net = t
            .app
            .overview
            .overview(None)
            .await
            .unwrap()
            .totals
            .net_worth;
        (
            state.members.len(),
            state.accounts.len(),
            state.holdings.len(),
            state.goals.len(),
            net,
        )
    }

    #[tokio::test]
    async fn import_creates_everything_and_resolves_references() {
        let t = TestApp::household().await;
        let counts = t.app.bundle.import(sample()).await.unwrap();
        assert_eq!(
            counts,
            ImportCounts {
                members: 2,
                accounts: 4,
                holdings: 1,
                goals: 1
            }
        );
        let accounts = t.app.accounts.list(None).await.unwrap();
        assert!(
            accounts
                .iter()
                .all(|a| a.account.source == AccountSource::Import)
        );
        let house = accounts
            .iter()
            .find(|a| a.account.external_id.as_deref() == Some("maison-1"))
            .unwrap();
        let loan = accounts
            .iter()
            .find(|a| a.account.external_id.as_deref() == Some("pret-1"))
            .unwrap();
        // Le prêt référence la maison alors qu'il la précède dans le bundle.
        assert_eq!(
            loan.account.loan.as_ref().unwrap().property_account_id,
            Some(house.account.id)
        );
        assert_eq!(house.owners.len(), 2);
        let goal = &t.app.goals.list().await.unwrap()[0];
        assert_eq!(goal.progress.current_amount, 5000.0);
        assert_eq!(goal.goal.member_ids, vec![t.alice]);
        // Bob existait : seule sa couleur est mise à jour, pas de doublon.
        let members = t.app.members.list().await.unwrap();
        assert_eq!(members.len(), 2);
        assert_eq!(members[1].color, "#abcdef");
    }

    #[tokio::test]
    async fn import_is_idempotent() {
        let t = TestApp::household().await;
        t.app.bundle.import(sample()).await.unwrap();
        let first = snapshot(&t).await;
        let ids: Vec<AccountId> = t.store.state().accounts.iter().map(|a| a.id).collect();
        for _ in 0..2 {
            assert_eq!(t.app.bundle.import(sample()).await.unwrap().accounts, 4);
        }
        assert_eq!(snapshot(&t).await, first);
        assert_eq!(
            t.store
                .state()
                .accounts
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            ids
        );
    }

    #[tokio::test]
    async fn export_then_import_elsewhere_reproduces_the_portfolio() {
        let source = TestApp::household().await;
        source.app.bundle.import(sample()).await.unwrap();
        let exported = source.app.bundle.export().await.unwrap();
        assert_eq!(exported.version, 1);
        assert_eq!(exported.accounts.len(), 4);
        let loan = exported
            .accounts
            .iter()
            .find(|a| a.external_id == "pret-1")
            .unwrap();
        assert_eq!(loan.property_external_id.as_deref(), Some("maison-1"));
        assert!(loan.holdings.is_none());
        assert_eq!(
            exported.goals[0].account_external_ids,
            vec!["livret-1".to_string()]
        );

        let target = TestApp::empty().await;
        target.app.bundle.import(exported.clone()).await.unwrap();
        let (a, b) = (snapshot(&source).await, snapshot(&target).await);
        assert_eq!(a, b);
        assert_eq!(target.app.bundle.export().await.unwrap(), exported);
        // Dans une base vide, le membre principal du bundle le reste.
        assert!(target.app.members.list().await.unwrap()[0].is_primary);
    }

    #[tokio::test]
    async fn export_of_manual_account_uses_its_id_and_reimport_matches_it() {
        let t = TestApp::household().await;
        let id = t.checking(&[(t.alice, 1.0)], 100.0).await;
        let exported = t.app.bundle.export().await.unwrap();
        assert_eq!(exported.accounts[0].external_id, id.to_string());
        t.app.bundle.import(exported).await.unwrap();
        let state = t.store.state();
        assert_eq!(state.accounts.len(), 1);
        // Un compte existant garde son origine et son absence d'identifiant externe.
        assert_eq!(
            (
                state.accounts[0].source,
                state.accounts[0].external_id.clone()
            ),
            (AccountSource::Manual, None)
        );
    }

    #[tokio::test]
    async fn provided_holdings_replace_existing_ones_and_absent_holdings_are_kept() {
        let t = TestApp::household().await;
        t.app.bundle.import(sample()).await.unwrap();
        let mut update = sample();
        update.accounts[1].holdings = Some(vec![
            holding_input("Obligation", 1.0, None, 100.0),
            holding_input("Action", 2.0, None, 50.0),
        ]);
        t.app.bundle.import(update).await.unwrap();
        assert_eq!(t.store.state().holdings.len(), 2);
        let mut untouched = sample();
        untouched.accounts[1].holdings = None;
        t.app.bundle.import(untouched).await.unwrap();
        assert_eq!(t.store.state().holdings.len(), 2);
    }

    #[tokio::test]
    async fn invalid_bundles_are_rejected_without_side_effects() {
        let t = TestApp::household().await;
        let before = t.store.state();
        let wrong_version = Bundle {
            version: 2,
            ..sample()
        };
        assert!(
            t.app
                .bundle
                .import(wrong_version)
                .await
                .unwrap_err()
                .to_string()
                .starts_with("Version de bundle")
        );

        let mut unknown_owner = sample();
        unknown_owner.accounts[3].owners[0].member = "Inconnu".into();
        assert_eq!(
            t.app
                .bundle
                .import(unknown_owner)
                .await
                .unwrap_err()
                .to_string(),
            "Membre inconnu dans le bundle : Inconnu"
        );

        let mut unknown_property = sample();
        unknown_property.accounts[2].property_external_id = Some("nulle-part".into());
        assert_eq!(
            t.app
                .bundle
                .import(unknown_property)
                .await
                .unwrap_err()
                .to_string(),
            "Compte inconnu dans le bundle : nulle-part"
        );

        let mut not_real_estate = sample();
        not_real_estate.accounts[2].property_external_id = Some("livret-1".into());
        assert!(t.app.bundle.import(not_real_estate).await.is_err());

        let mut no_external = sample();
        no_external.accounts[0].external_id = "  ".into();
        assert!(t.app.bundle.import(no_external).await.is_err());

        let mut bad_goal = sample();
        bad_goal.goals[0].account_external_ids = vec!["absent".into()];
        assert!(t.app.bundle.import(bad_goal).await.is_err());

        let after = t.store.state();
        assert_eq!(
            (after.accounts.len(), after.goals.len(), after.members),
            (before.accounts.len(), 0, before.members)
        );
    }

    #[tokio::test]
    async fn primary_role_is_only_taken_when_vacant() {
        let t = TestApp::household().await;
        let bundle = Bundle {
            version: 1,
            members: vec![BundleMember {
                is_primary: Some(true),
                ..member("Chloé")
            }],
            accounts: vec![],
            goals: vec![],
        };
        t.app.bundle.import(bundle.clone()).await.unwrap();
        let members = t.app.members.list().await.unwrap();
        assert_eq!(members.iter().filter(|m| m.is_primary).count(), 1);
        assert!(
            members
                .iter()
                .find(|m| m.name == "Alice")
                .unwrap()
                .is_primary
        );

        let empty = TestApp::empty().await;
        empty.app.bundle.import(bundle).await.unwrap();
        assert!(empty.app.members.list().await.unwrap()[0].is_primary);
    }

    #[tokio::test]
    async fn goals_are_matched_by_name() {
        let t = TestApp::household().await;
        t.app.bundle.import(sample()).await.unwrap();
        let mut changed = sample();
        changed.goals[0].goal.coverage_months = Some(Some(12.0));
        changed.goals[0].goal.target_date = Some(Some(date(2030, 1, 1)));
        t.app.bundle.import(changed).await.unwrap();
        let goals = t.app.goals.list().await.unwrap();
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].goal.coverage_months, Some(12.0));
    }
}
