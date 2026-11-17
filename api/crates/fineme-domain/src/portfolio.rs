//! Photographie du patrimoine à une date et calculs associés : valorisation des comptes,
//! totaux par vue (foyer / membre), vue d'ensemble et synthèse immobilière.

use std::collections::HashMap;

use chrono::NaiveDate;

use crate::account::{Account, AccountKind};
use crate::fx::FxRates;
use crate::holding::{Holding, HoldingValuation};
use crate::ids::{AccountId, MemberId};
use crate::loan::{Loan, LoanSummary};
use crate::member::Member;
use crate::ownership::View;
use crate::property::Property;

const TOP_HOLDINGS: usize = 10;
/// Regroupement des comptes sans établissement.
pub const NO_INSTITUTION: &str = "Autre";

#[derive(Debug, Clone)]
pub struct Portfolio {
    pub members: Vec<Member>,
    pub accounts: Vec<Account>,
    pub holdings: HashMap<AccountId, Vec<Holding>>,
    pub rates: FxRates,
    pub today: NaiveDate,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Totals {
    pub gross_assets: f64,
    pub liabilities: f64,
    pub net_worth: f64,
}

/// Valeur totale d'un compte en EUR et prix de revient s'il est connu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccountValuation {
    pub value: f64,
    pub invested: Option<f64>,
}

impl AccountValuation {
    pub fn gain(&self) -> Option<f64> {
        self.invested.map(|invested| self.value - invested)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OwnerShare {
    pub member_id: MemberId,
    pub name: String,
    pub share: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountView {
    pub account: Account,
    pub owners: Vec<OwnerShare>,
    pub valuation: AccountValuation,
    /// `value` × part(s) concernée(s) par la vue.
    pub owned_value: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HoldingView {
    pub holding: Holding,
    pub valuation: HoldingValuation,
    /// Cours automatique qui n'est plus mis à jour (voir [`Holding::price_is_stale`]).
    pub price_stale: bool,
}

impl HoldingView {
    pub fn new(holding: Holding, rates: &FxRates, today: NaiveDate) -> Self {
        Self {
            valuation: holding.valuation(rates),
            price_stale: holding.price_is_stale(today),
            holding,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LoanView {
    pub account_id: AccountId,
    pub loan: Loan,
    pub summary: LoanSummary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccountDetail {
    pub view: AccountView,
    pub holdings: Vec<HoldingView>,
    pub loan: Option<LoanView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AllocationSlice {
    pub kind: AccountKind,
    pub value: f64,
    pub pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MemberTotals {
    pub member: Member,
    pub totals: Totals,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopHolding {
    pub name: String,
    pub symbol: Option<String>,
    pub value: f64,
    pub gain_pct: Option<f64>,
    pub account_name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Overview {
    pub totals: Totals,
    pub financial_assets: f64,
    pub invested: f64,
    pub gain: f64,
    pub allocation: Vec<AllocationSlice>,
    pub by_institution: Vec<(String, f64)>,
    pub by_envelope: Vec<(String, f64)>,
    pub members: Vec<MemberTotals>,
    pub top_holdings: Vec<TopHolding>,
    /// Frais courants moyens pondérés par la valeur des positions (fraction).
    pub weighted_expense_ratio: Option<f64>,
}

/// Ligne d'instantané : valeur d'un compte détenue dans une vue, à une date.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountSnapshot {
    pub account_id: AccountId,
    /// Nom au jour de l'instantané : l'historique survit au renommage et à la suppression.
    pub name: String,
    pub kind: AccountKind,
    /// Valeur détenue en EUR, positive y compris pour un prêt.
    pub value: f64,
    /// Plus-value latente détenue des positions au PRU connu ; `None` si aucune.
    pub gain: Option<f64>,
}

/// Photographie d'une vue : totaux et détail par compte.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub totals: Totals,
    pub accounts: Vec<AccountSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkedLoan {
    pub name: String,
    pub view: LoanView,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealEstateItem {
    pub account: AccountView,
    pub property: Property,
    pub loans: Vec<LinkedLoan>,
    pub equity: f64,
    pub gross_yield_pct: Option<f64>,
    pub net_yield_pct: Option<f64>,
    pub latent_gain: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RealEstateTotals {
    pub value: f64,
    pub remaining_debt: f64,
    pub equity: f64,
    pub monthly_rent: f64,
    pub monthly_payments: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RealEstateReport {
    pub items: Vec<RealEstateItem>,
    pub totals: RealEstateTotals,
}

fn add_to_group(groups: &mut Vec<(String, f64)>, name: &str, value: f64) {
    match groups.iter_mut().find(|(existing, _)| existing == name) {
        Some(group) => group.1 += value,
        None => groups.push((name.to_string(), value)),
    }
}

fn sorted_desc(mut groups: Vec<(String, f64)>) -> Vec<(String, f64)> {
    groups.sort_by(|a, b| b.1.total_cmp(&a.1));
    groups
}

impl Portfolio {
    pub fn member(&self, id: MemberId) -> Option<&Member> {
        self.members.iter().find(|member| member.id == id)
    }

    pub fn account(&self, id: AccountId) -> Option<&Account> {
        self.accounts.iter().find(|account| account.id == id)
    }

    pub fn holdings_of(&self, account_id: AccountId) -> &[Holding] {
        self.holdings.get(&account_id).map_or(&[], Vec::as_slice)
    }

    /// Valeur d'un compte : bien immobilier, capital restant dû, ou espèces + positions.
    pub fn valuation(&self, account: &Account) -> AccountValuation {
        match account.kind {
            AccountKind::RealEstate => match &account.property {
                Some(property) => AccountValuation {
                    value: self.rates.to_eur(property.current_value, &account.currency),
                    invested: Some(self.rates.to_eur(property.cost(), &account.currency)),
                },
                None => AccountValuation {
                    value: 0.0,
                    invested: None,
                },
            },
            AccountKind::Loan => {
                let remaining = account
                    .loan
                    .as_ref()
                    .map_or(0.0, |loan| loan.terms.remaining_capital(self.today));
                AccountValuation {
                    value: self.rates.to_eur(remaining, &account.currency),
                    invested: None,
                }
            }
            _ => {
                let cash = self.rates.to_eur(account.cash_balance, &account.currency);
                let valuations: Vec<HoldingValuation> = self
                    .holdings_of(account.id)
                    .iter()
                    .map(|holding| holding.valuation(&self.rates))
                    .collect();
                let value = cash + valuations.iter().map(|v| v.value).sum::<f64>();
                // Prix de revient connu seulement si toutes les positions ont un PRU.
                let invested = (!valuations.is_empty()
                    && valuations.iter().all(|v| v.invested.is_some()))
                .then(|| cash + valuations.iter().filter_map(|v| v.invested).sum::<f64>());
                AccountValuation { value, invested }
            }
        }
    }

    pub fn owned_value(&self, account: &Account, view: View) -> f64 {
        self.valuation(account).value * account.owners.fraction(view)
    }

    pub fn account_view(&self, account: &Account, view: View) -> AccountView {
        let valuation = self.valuation(account);
        let owners = account
            .owners
            .owners()
            .iter()
            .map(|(member_id, share)| OwnerShare {
                member_id: *member_id,
                name: self
                    .member(*member_id)
                    .map(|member| member.name.clone())
                    .unwrap_or_default(),
                share: share.value(),
            })
            .collect();
        AccountView {
            owned_value: valuation.value * account.owners.fraction(view),
            account: account.clone(),
            owners,
            valuation,
        }
    }

    pub fn loan_view(&self, account: &Account) -> Option<LoanView> {
        account.loan.as_ref().map(|loan| LoanView {
            account_id: account.id,
            loan: loan.clone(),
            summary: loan.terms.summary(self.today),
        })
    }

    pub fn holding_views(&self, account_id: AccountId) -> Vec<HoldingView> {
        self.holdings_of(account_id)
            .iter()
            .map(|holding| HoldingView::new(holding.clone(), &self.rates, self.today))
            .collect()
    }

    pub fn account_detail(&self, account: &Account, view: View) -> AccountDetail {
        AccountDetail {
            view: self.account_view(account, view),
            holdings: self.holding_views(account.id),
            loan: self.loan_view(account),
        }
    }

    /// Comptes visibles dans la vue : tous pour le foyer, ceux dont il détient une part pour un membre.
    pub fn accounts_in_view(&self, view: View) -> impl Iterator<Item = &Account> {
        self.accounts.iter().filter(move |account| match view {
            View::Household => true,
            View::Member(_) => account.owners.fraction(view) > 0.0,
        })
    }

    /// Actifs bruts, dettes et patrimoine net pour une vue.
    pub fn totals(&self, view: View) -> Totals {
        let mut totals = Totals::default();
        for account in &self.accounts {
            let owned = self.owned_value(account, view);
            if account.kind.is_liability() {
                totals.liabilities += owned;
            } else {
                totals.gross_assets += owned;
            }
        }
        totals.net_worth = totals.gross_assets - totals.liabilities;
        totals
    }

    /// Valeur détenue par le foyer dans les comptes donnés, hors dettes (avancement des objectifs).
    pub fn household_value_of(&self, account_ids: &[AccountId]) -> f64 {
        let sum: f64 = self
            .accounts
            .iter()
            .filter(|account| account_ids.contains(&account.id) && !account.kind.is_liability())
            .map(|account| self.owned_value(account, View::Household))
            .sum();
        sum + 0.0 // une somme vide vaut -0.0
    }

    /// Plus-value latente (100 %) des positions dont le PRU est connu ; `None` si aucune.
    fn holdings_gain(&self, account: &Account) -> Option<f64> {
        if !account.kind.can_hold_positions() {
            return None;
        }
        let gains: Vec<f64> = self
            .holdings_of(account.id)
            .iter()
            .filter_map(|holding| holding.valuation(&self.rates).gain)
            .collect();
        (!gains.is_empty()).then(|| gains.iter().sum())
    }

    /// Instantané d'une vue : totaux et valeur détenue de chaque compte de la vue.
    pub fn snapshot(&self, view: View) -> Snapshot {
        let accounts = self
            .accounts_in_view(view)
            .map(|account| {
                let fraction = account.owners.fraction(view);
                AccountSnapshot {
                    account_id: account.id,
                    name: account.name.clone(),
                    kind: account.kind,
                    value: self.valuation(account).value * fraction,
                    gain: self.holdings_gain(account).map(|gain| gain * fraction),
                }
            })
            .collect();
        Snapshot {
            totals: self.totals(view),
            accounts,
        }
    }

    pub fn overview(&self, view: View) -> Overview {
        let totals = self.totals(view);
        let mut by_kind: Vec<(AccountKind, f64)> = AccountKind::ALL
            .iter()
            .filter(|kind| !kind.is_liability())
            .map(|kind| (*kind, 0.0))
            .collect();
        let (mut financial_assets, mut invested, mut gain) = (0.0, 0.0, 0.0);
        let (mut institutions, mut envelopes) = (Vec::new(), Vec::new());
        let mut top = Vec::new();
        let (mut fee_weight, mut fee_sum) = (0.0, 0.0);

        for account in &self.accounts {
            let fraction = account.owners.fraction(view);
            if account.kind.is_liability() || fraction <= 0.0 {
                continue;
            }
            let owned = self.valuation(account).value * fraction;
            if let Some(slot) = by_kind.iter_mut().find(|(kind, _)| *kind == account.kind) {
                slot.1 += owned;
            }
            if account.kind.is_financial() {
                financial_assets += owned;
            }
            add_to_group(
                &mut institutions,
                account.institution.as_deref().unwrap_or(NO_INSTITUTION),
                owned,
            );
            if let Some(envelope) = &account.envelope {
                add_to_group(&mut envelopes, envelope, owned);
            }
            if !account.kind.can_hold_positions() {
                continue;
            }
            for holding in self.holdings_of(account.id) {
                let valuation = holding.valuation(&self.rates);
                let value = valuation.value * fraction;
                if let (Some(cost), Some(holding_gain)) = (valuation.invested, valuation.gain) {
                    invested += cost * fraction;
                    gain += holding_gain * fraction;
                }
                if let Some(ratio) = holding.expense_ratio {
                    fee_weight += value;
                    fee_sum += value * ratio;
                }
                top.push(TopHolding {
                    name: holding.name.clone(),
                    symbol: holding.symbol.clone(),
                    value,
                    gain_pct: valuation.gain_pct,
                    account_name: account.name.clone(),
                });
            }
        }
        top.sort_by(|a, b| b.value.total_cmp(&a.value));
        top.truncate(TOP_HOLDINGS);

        let gross = totals.gross_assets;
        let allocation = by_kind
            .into_iter()
            .filter(|(_, value)| *value != 0.0)
            .map(|(kind, value)| AllocationSlice {
                kind,
                value,
                pct: if gross > 0.0 {
                    value / gross * 100.0
                } else {
                    0.0
                },
            })
            .collect();

        Overview {
            totals,
            financial_assets,
            invested,
            gain,
            allocation,
            by_institution: sorted_desc(institutions),
            by_envelope: sorted_desc(envelopes),
            members: self
                .members
                .iter()
                .map(|member| MemberTotals {
                    member: member.clone(),
                    totals: self.totals(View::Member(member.id)),
                })
                .collect(),
            top_holdings: top,
            weighted_expense_ratio: (fee_weight > 0.0).then(|| fee_sum / fee_weight),
        }
    }

    /// Synthèse immobilière ; valeur, dette, loyers et mensualités sont pondérés par la vue
    /// (part du bien pour la valeur et le loyer, part du compte de prêt pour la dette et les mensualités).
    pub fn real_estate(&self, view: View) -> RealEstateReport {
        let mut items = Vec::new();
        let mut totals = RealEstateTotals::default();
        for account in self
            .accounts_in_view(view)
            .filter(|account| account.kind == AccountKind::RealEstate)
        {
            let Some(property) = &account.property else {
                continue;
            };
            let fraction = account.owners.fraction(view);
            let account_view = self.account_view(account, view);
            let mut debt = 0.0;
            let mut loans = Vec::new();
            for loan_account in &self.accounts {
                let Some(loan_view) = self.loan_view(loan_account) else {
                    continue;
                };
                if loan_view.loan.property_account_id != Some(account.id) {
                    continue;
                }
                let loan_fraction = loan_account.owners.fraction(view);
                debt += self
                    .rates
                    .to_eur(loan_view.summary.remaining_capital, &loan_account.currency)
                    * loan_fraction;
                if loan_view.summary.remaining_capital > 0.0 {
                    let payment =
                        loan_view.summary.monthly_payment + loan_view.summary.monthly_insurance;
                    totals.monthly_payments +=
                        self.rates.to_eur(payment, &loan_account.currency) * loan_fraction;
                }
                loans.push(LinkedLoan {
                    name: loan_account.name.clone(),
                    view: loan_view,
                });
            }
            loans.sort_by(|a, b| a.name.cmp(&b.name));
            let (gross_yield_pct, net_yield_pct) = property.rental_yields();
            totals.value += account_view.owned_value;
            totals.remaining_debt += debt;
            totals.monthly_rent +=
                self.rates.to_eur(property.monthly_rent, &account.currency) * fraction;
            items.push(RealEstateItem {
                equity: account_view.owned_value - debt,
                latent_gain: account_view.valuation.gain().unwrap_or(0.0) * fraction,
                account: account_view,
                property: property.clone(),
                loans,
                gross_yield_pct,
                net_yield_pct,
            });
        }
        totals.equity = totals.value - totals.remaining_debt;
        RealEstateReport { items, totals }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AccountSource;
    use crate::fx::Currency;
    use crate::holding::{HoldingKind, PriceSource};
    use crate::ids::{HoldingId, LoanId, PropertyId};
    use crate::loan::LoanTerms;
    use crate::ownership::Ownership;
    use crate::property::{BuildingType, PropertyCategory};
    use chrono::{DateTime, Utc};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    fn member(name: &str, is_primary: bool) -> Member {
        Member {
            id: MemberId::generate(),
            name: name.into(),
            color: "#000000".into(),
            is_primary,
        }
    }

    fn account(kind: AccountKind, name: &str, cash: f64, owners: &[(MemberId, f64)]) -> Account {
        Account {
            id: AccountId::generate(),
            name: name.into(),
            institution: Some("Banque Fictive".into()),
            kind,
            envelope: None,
            currency: Currency::eur(),
            cash_balance: cash,
            annual_yield_pct: None,
            source: AccountSource::Manual,
            external_id: None,
            connection_id: None,
            opened_at: None,
            updated_at: DateTime::<Utc>::UNIX_EPOCH,
            owners: Ownership::from_fractions(owners.iter().copied()).unwrap(),
            property: None,
            loan: None,
        }
    }

    struct Fixture {
        portfolio: Portfolio,
        alice: MemberId,
        bob: MemberId,
        house: AccountId,
        loan: AccountId,
        pea: AccountId,
        checking: AccountId,
    }

    /// Foyer fictif : Alice et Bob ; maison 300 k€ (louée 1 200 €/mois) et prêt 200 k€ non démarré, à 50/50 ;
    /// PEA d'Alice (1 000 € d'espèces + 10 parts à 125 $, PRU 100 $, 1 EUR = 1,25 USD) ;
    /// compte courant commun 60/20 (les parts ne totalisent pas 100 %).
    fn fixture() -> Fixture {
        let (alice, bob) = (member("Alice", true), member("Bob", false));
        let (a, b) = (alice.id, bob.id);
        let mut house = account(
            AccountKind::RealEstate,
            "Maison",
            0.0,
            &[(a, 0.5), (b, 0.5)],
        );
        house.institution = None;
        house.property = Some(Property {
            id: PropertyId::generate(),
            address: None,
            category: PropertyCategory::Rental,
            building_type: BuildingType::House,
            surface_m2: None,
            purchase_price: 250_000.0,
            purchase_fees: 20_000.0,
            purchased_at: None,
            current_value: 300_000.0,
            monthly_rent: 1200.0,
            monthly_charges: 100.0,
            yearly_property_tax: 1200.0,
        });
        let mut loan = account(AccountKind::Loan, "Prêt maison", 0.0, &[(a, 0.5), (b, 0.5)]);
        loan.loan = Some(Loan {
            id: LoanId::generate(),
            property_account_id: Some(house.id),
            terms: LoanTerms::new(200_000.0, 4.0, 0.3, 240, d(2026, 6, 1)).unwrap(),
        });
        let mut pea = account(AccountKind::Investment, "PEA", 1000.0, &[(a, 1.0)]);
        pea.envelope = Some("pea".into());
        let checking = account(
            AccountKind::Checking,
            "Courant",
            1000.0,
            &[(a, 0.6), (b, 0.2)],
        );
        let etf = Holding {
            id: HoldingId::generate(),
            account_id: pea.id,
            kind: HoldingKind::Security,
            name: "ETF Monde".into(),
            isin: None,
            symbol: Some("WLD".into()),
            security_type: Some("etf".into()),
            quantity: 10.0,
            buy_price: Some(100.0),
            current_price: 125.0,
            currency: Currency::parse("USD").unwrap(),
            expense_ratio: Some(0.002),
            price_source: PriceSource::Manual,
            price_updated_at: None,
        };
        Fixture {
            alice: a,
            bob: b,
            house: house.id,
            loan: loan.id,
            pea: pea.id,
            checking: checking.id,
            portfolio: Portfolio {
                holdings: HashMap::from([(pea.id, vec![etf])]),
                rates: FxRates::new([(Currency::parse("USD").unwrap(), 1.25)]),
                members: vec![alice, bob],
                accounts: vec![house, loan, pea, checking],
                today: d(2026, 1, 1),
            },
        }
    }

    fn view_of(f: &Fixture, id: AccountId, view: View) -> AccountView {
        f.portfolio
            .account_view(f.portfolio.account(id).unwrap(), view)
    }

    #[test]
    fn household_totals_sum_all_shares() {
        let f = fixture();
        let t = f.portfolio.totals(View::Household);
        // maison 300 000 + PEA 2 000 + courant 1 000 × 0,8 (20 % détenus hors foyer)
        assert!(close(t.gross_assets, 302_800.0));
        assert!(close(t.liabilities, 200_000.0));
        assert!(close(t.net_worth, 102_800.0));
    }

    #[test]
    fn member_totals_use_member_share_only() {
        let f = fixture();
        let a = f.portfolio.totals(View::Member(f.alice));
        assert!(close(a.gross_assets, 150_000.0 + 2000.0 + 600.0));
        assert!(close(a.liabilities, 100_000.0));
        let b = f.portfolio.totals(View::Member(f.bob));
        assert!(close(b.gross_assets, 150_000.0 + 200.0));
        assert!(close(b.net_worth, 50_200.0));
        assert!(close(
            a.net_worth + b.net_worth,
            f.portfolio.totals(View::Household).net_worth
        ));
    }

    fn line(snapshot: &Snapshot, id: AccountId) -> &AccountSnapshot {
        snapshot
            .accounts
            .iter()
            .find(|line| line.account_id == id)
            .unwrap()
    }

    #[test]
    fn household_snapshot_details_every_account_consistently_with_totals() {
        let f = fixture();
        let s = f.portfolio.snapshot(View::Household);
        assert_eq!(s.totals, f.portfolio.totals(View::Household));
        assert_eq!(s.accounts.len(), 4);
        let assets: f64 = s
            .accounts
            .iter()
            .filter(|l| !l.kind.is_liability())
            .map(|l| l.value)
            .sum();
        let debts: f64 = s
            .accounts
            .iter()
            .filter(|l| l.kind.is_liability())
            .map(|l| l.value)
            .sum();
        assert!(close(assets, s.totals.gross_assets));
        assert!(close(debts, s.totals.liabilities));
        // Parts < 100 % : 80 % du compte courant.
        assert!(close(line(&s, f.checking).value, 800.0));
        // 10 parts × (125 − 100) $ à 1,25 $/€.
        let pea = line(&s, f.pea);
        assert_eq!(pea.name, "PEA");
        assert_eq!(pea.kind, AccountKind::Investment);
        assert!(close(pea.gain.unwrap(), 200.0));
        assert!(close(
            s.accounts.iter().filter_map(|l| l.gain).sum::<f64>(),
            f.portfolio.overview(View::Household).gain
        ));
        // Sans positions, pas de plus-value : ni l'immobilier, ni le prêt, ni le compte courant.
        assert_eq!(line(&s, f.house).gain, None);
        assert_eq!(line(&s, f.loan).gain, None);
        assert_eq!(line(&s, f.checking).gain, None);
    }

    #[test]
    fn member_snapshot_keeps_only_owned_accounts_at_the_member_share() {
        let f = fixture();
        let bob = f.portfolio.snapshot(View::Member(f.bob));
        assert_eq!(bob.totals, f.portfolio.totals(View::Member(f.bob)));
        // Le PEA d'Alice n'apparaît pas chez Bob.
        assert!(bob.accounts.iter().all(|l| l.account_id != f.pea));
        assert_eq!(bob.accounts.len(), 3);
        assert!(close(line(&bob, f.checking).value, 200.0));
        assert!(close(line(&bob, f.loan).value, 100_000.0));
        let alice = f.portfolio.snapshot(View::Member(f.alice));
        assert!(close(line(&alice, f.pea).gain.unwrap(), 200.0));
        assert!(
            f.portfolio
                .snapshot(View::Member(MemberId::generate()))
                .accounts
                .is_empty()
        );
    }

    #[test]
    fn unknown_member_owns_nothing() {
        let f = fixture();
        assert_eq!(
            f.portfolio.totals(View::Member(MemberId::generate())),
            Totals::default()
        );
    }

    #[test]
    fn real_estate_account_value_and_cost() {
        let f = fixture();
        let house = view_of(&f, f.house, View::Member(f.alice));
        assert_eq!(house.valuation.value, 300_000.0);
        assert_eq!(house.owned_value, 150_000.0);
        assert_eq!(house.valuation.invested, Some(270_000.0));
        assert_eq!(house.valuation.gain(), Some(30_000.0));
        assert_eq!(house.owners.len(), 2);
        assert_eq!(house.owners[0].name, "Alice");
    }

    #[test]
    fn loan_value_is_remaining_capital() {
        let mut f = fixture();
        // Avant le début du prêt : tout le capital.
        assert_eq!(
            view_of(&f, f.loan, View::Household).valuation.value,
            200_000.0
        );
        f.portfolio.today = d(2030, 1, 1);
        let later = view_of(&f, f.loan, View::Household).valuation.value;
        assert!(later < 200_000.0 && later > 0.0);
        f.portfolio.today = d(2047, 1, 1);
        assert_eq!(view_of(&f, f.loan, View::Household).valuation.value, 0.0);
    }

    #[test]
    fn investment_account_value_converts_holdings_to_eur() {
        let f = fixture();
        let pea = view_of(&f, f.pea, View::Household);
        assert!(close(pea.valuation.value, 2000.0));
        assert!(close(pea.valuation.invested.unwrap(), 1800.0));
        assert!(close(pea.valuation.gain().unwrap(), 200.0));
    }

    #[test]
    fn cost_basis_unknown_without_holdings_or_with_a_missing_buy_price() {
        let mut f = fixture();
        assert_eq!(
            view_of(&f, f.checking, View::Household).valuation.invested,
            None
        );
        f.portfolio.holdings.get_mut(&f.pea).unwrap()[0].buy_price = None;
        assert_eq!(view_of(&f, f.pea, View::Household).valuation.invested, None);
    }

    #[test]
    fn missing_fx_rate_falls_back_to_face_value() {
        let mut f = fixture();
        f.portfolio.rates = FxRates::default();
        assert!(close(
            view_of(&f, f.pea, View::Household).valuation.value,
            1000.0 + 1250.0
        ));
    }

    #[test]
    fn overview_breakdown_for_household() {
        let f = fixture();
        let o = f.portfolio.overview(View::Household);
        assert!(close(o.financial_assets, 2000.0));
        assert!(close(o.invested, 800.0));
        assert!(close(o.gain, 200.0));
        assert_eq!(o.weighted_expense_ratio, Some(0.002));
        assert_eq!(o.top_holdings.len(), 1);
        assert!(close(o.top_holdings[0].gain_pct.unwrap(), 25.0));
        assert_eq!(o.members.len(), 2);
        assert!(close(
            o.allocation.iter().map(|slice| slice.pct).sum::<f64>(),
            100.0
        ));
        assert!(o.allocation.iter().all(|slice| !slice.kind.is_liability()));
        assert_eq!(o.by_envelope, vec![("pea".to_string(), 2000.0)]);
        // Trié par valeur décroissante ; la maison sans établissement tombe dans « Autre ».
        assert_eq!(o.by_institution[0].0, NO_INSTITUTION);
        assert!(close(o.by_institution[1].1, 2800.0));
    }

    #[test]
    fn overview_for_member_without_positions() {
        let f = fixture();
        let o = f.portfolio.overview(View::Member(f.bob));
        assert_eq!(o.financial_assets, 0.0);
        assert!(o.top_holdings.is_empty());
        assert_eq!(o.weighted_expense_ratio, None);
        assert!(close(o.totals.net_worth, 50_200.0));
        // La liste des membres reste complète quelle que soit la vue.
        assert_eq!(o.members.len(), 2);
    }

    #[test]
    fn top_holdings_are_capped_and_sorted() {
        let mut f = fixture();
        let template = f.portfolio.holdings[&f.pea][0].clone();
        let many: Vec<Holding> = (1..=12)
            .map(|i| Holding {
                id: HoldingId::generate(),
                name: format!("Titre {i}"),
                quantity: f64::from(i),
                ..template.clone()
            })
            .collect();
        f.portfolio.holdings.insert(f.pea, many);
        let top = f.portfolio.overview(View::Household).top_holdings;
        assert_eq!(top.len(), 10);
        assert_eq!(top[0].name, "Titre 12");
        assert!(top.windows(2).all(|w| w[0].value >= w[1].value));
    }

    #[test]
    fn accounts_in_view_filters_for_member_only() {
        let f = fixture();
        assert_eq!(f.portfolio.accounts_in_view(View::Household).count(), 4);
        let bob: Vec<&str> = f
            .portfolio
            .accounts_in_view(View::Member(f.bob))
            .map(|a| a.name.as_str())
            .collect();
        assert_eq!(bob, vec!["Maison", "Prêt maison", "Courant"]);
    }

    #[test]
    fn household_value_of_linked_accounts_excludes_debts() {
        let f = fixture();
        assert!(close(
            f.portfolio.household_value_of(&[f.pea, f.checking, f.loan]),
            2800.0
        ));
        let empty = f.portfolio.household_value_of(&[]);
        assert!(empty == 0.0 && empty.is_sign_positive());
    }

    #[test]
    fn real_estate_report_household() {
        let f = fixture();
        let report = f.portfolio.real_estate(View::Household);
        assert_eq!(report.items.len(), 1);
        let item = &report.items[0];
        assert_eq!(item.loans.len(), 1);
        assert_eq!(item.loans[0].name, "Prêt maison");
        assert!(close(item.equity, 100_000.0));
        assert!(close(item.latent_gain, 30_000.0));
        assert!(close(
            item.gross_yield_pct.unwrap(),
            1200.0 * 12.0 / 270_000.0 * 100.0
        ));
        assert!(close(report.totals.value, 300_000.0));
        assert!(close(report.totals.remaining_debt, 200_000.0));
        assert!(close(report.totals.equity, 100_000.0));
        assert!(close(report.totals.monthly_rent, 1200.0));
        let terms = item.loans[0].view.loan.terms;
        assert!(close(
            report.totals.monthly_payments,
            terms.monthly_payment() + terms.monthly_insurance()
        ));
    }

    #[test]
    fn real_estate_report_member_is_weighted_by_shares() {
        let f = fixture();
        let report = f.portfolio.real_estate(View::Member(f.alice));
        assert!(close(report.totals.value, 150_000.0));
        assert!(close(report.totals.remaining_debt, 100_000.0));
        assert!(close(report.totals.monthly_rent, 600.0));
        assert!(close(report.items[0].latent_gain, 15_000.0));
        assert!(
            f.portfolio
                .real_estate(View::Member(MemberId::generate()))
                .items
                .is_empty()
        );
    }

    #[test]
    fn repaid_loans_no_longer_count_in_monthly_payments() {
        let mut f = fixture();
        f.portfolio.today = d(2047, 1, 1);
        let report = f.portfolio.real_estate(View::Household);
        assert_eq!(report.totals.monthly_payments, 0.0);
        assert_eq!(report.totals.remaining_debt, 0.0);
        assert_eq!(report.items[0].loans.len(), 1);
    }

    #[test]
    fn account_detail_bundles_holdings_and_loan() {
        let f = fixture();
        let pea = f
            .portfolio
            .account_detail(f.portfolio.account(f.pea).unwrap(), View::Household);
        assert_eq!(pea.holdings.len(), 1);
        assert!(pea.loan.is_none());
        let loan = f
            .portfolio
            .account_detail(f.portfolio.account(f.loan).unwrap(), View::Household);
        assert_eq!(loan.loan.unwrap().summary.remaining_capital, 200_000.0);
    }
}
