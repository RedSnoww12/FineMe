//! Faux ports en mémoire et fixtures (fictives) pour tester les cas d'usage sans base ni réseau.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use fineme_domain::account::Account;
use fineme_domain::alert::{Alert, AlertRule};
use fineme_domain::budget::{BudgetTarget, CategoryRule};
use fineme_domain::connection::Connection;
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::goal::Goal;
use fineme_domain::history::DateRange;
use fineme_domain::holding::{Holding, HoldingKind, PriceSource};
use fineme_domain::ids::{
    AccountId, AlertId, BudgetRuleId, ConnectionId, GoalId, HoldingId, MemberId, OperationId,
    TransactionId, UserId,
};
use fineme_domain::login::LoginEvent;
use fineme_domain::member::Member;
use fineme_domain::operation::Operation;
use fineme_domain::performance::Benchmark;
use fineme_domain::portfolio::{Snapshot, Totals};
use fineme_domain::transaction::Transaction;
use uuid::Uuid;

use crate::app::{App, Ports, Settings};
use crate::commands::{AccountInput, HoldingInput, LoanInput, OwnerInput, PropertyInput};
use crate::ports::*;

// ---------------------------------------------------------------- stockage en mémoire

#[derive(Debug, Clone, Default)]
pub struct State {
    pub users: Vec<Credentials>,
    pub members: Vec<Member>,
    pub accounts: Vec<Account>,
    pub holdings: Vec<Holding>,
    pub transactions: Vec<Transaction>,
    pub rules: Vec<CategoryRule>,
    pub targets: Vec<BudgetTarget>,
    pub operations: Vec<Operation>,
    /// Instantanés enregistrés, par vue (`None` = foyer).
    pub snapshots: Vec<(Option<MemberId>, SnapshotPoint)>,
    pub price_history: Vec<(HoldingId, PricePoint)>,
    pub benchmark: Option<Benchmark>,
    pub benchmark_prices: Vec<(NaiveDate, f64)>,
    pub goals: Vec<Goal>,
    pub connections: Vec<Connection>,
    pub alerts: Vec<Alert>,
    pub alert_rules: Vec<AlertRule>,
    pub rates: Vec<(Currency, f64)>,
    pub rates_updated_at: Option<DateTime<Utc>>,
    pub fail_price_updates: bool,
    pub login_events: Vec<LoginEvent>,
}

impl State {
    fn save_account(&mut self, write: &AccountWrite) {
        match self
            .accounts
            .iter_mut()
            .find(|account| account.id == write.account.id)
        {
            Some(existing) => *existing = write.account.clone(),
            None => self.accounts.push(write.account.clone()),
        }
        if let Some(holdings) = &write.holdings {
            self.holdings
                .retain(|holding| holding.account_id != write.account.id);
            self.holdings.extend(holdings.iter().cloned());
            self.drop_orphan_operations();
        }
    }

    /// Comme la base : les opérations d'un compte ou d'une position supprimés disparaissent.
    fn drop_orphan_operations(&mut self) {
        let accounts: Vec<AccountId> = self.accounts.iter().map(|a| a.id).collect();
        let holdings: Vec<HoldingId> = self.holdings.iter().map(|h| h.id).collect();
        self.operations.retain(|operation| {
            accounts.contains(&operation.account_id)
                && operation
                    .movement
                    .holding()
                    .is_none_or(|holding| holdings.contains(&holding))
        });
    }

    fn touch(&mut self, account_id: AccountId, at: DateTime<Utc>) {
        if let Some(account) = self
            .accounts
            .iter_mut()
            .find(|account| account.id == account_id)
        {
            account.updated_at = at;
        }
    }
}

#[derive(Default)]
pub struct InMemoryStore(Mutex<State>);

impl InMemoryStore {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }

    pub fn state(&self) -> State {
        self.lock().clone()
    }

    /// Simule la disparition de l'utilisateur (base restaurée, ligne supprimée à la main).
    pub fn remove_users(&self) {
        self.lock().users.clear();
    }

    pub fn fail_price_updates(&self) {
        self.lock().fail_price_updates = true;
    }

    pub fn set_price_auto(&self, id: HoldingId, price: f64) {
        let mut state = self.lock();
        let holding = state
            .holdings
            .iter_mut()
            .find(|holding| holding.id == id)
            .unwrap();
        holding.current_price = price;
        holding.price_source = PriceSource::Auto;
    }
}

#[async_trait]
impl LoginJournal for InMemoryStore {
    async fn record(&self, event: &LoginEvent) -> RepoResult<()> {
        self.lock().login_events.push(event.clone());
        Ok(())
    }
    async fn recent(&self, limit: i64) -> RepoResult<Vec<LoginEvent>> {
        let mut events = self.lock().login_events.clone();
        events.sort_by_key(|event| std::cmp::Reverse(event.at));
        events.truncate(usize::try_from(limit).unwrap_or(0));
        Ok(events)
    }
    async fn prune_before(&self, before: DateTime<Utc>) -> RepoResult<u64> {
        let mut state = self.lock();
        let count = state.login_events.len();
        state.login_events.retain(|event| event.at >= before);
        Ok((count - state.login_events.len()) as u64)
    }
}

#[async_trait]
impl UserRepository for InMemoryStore {
    async fn exists_any(&self) -> RepoResult<bool> {
        Ok(!self.lock().users.is_empty())
    }
    async fn find_by_email(&self, email: &str) -> RepoResult<Option<Credentials>> {
        Ok(self
            .lock()
            .users
            .iter()
            .find(|c| c.user.email == email)
            .cloned())
    }
    async fn find_by_id(&self, id: UserId) -> RepoResult<Option<Credentials>> {
        Ok(self.lock().users.iter().find(|c| c.user.id == id).cloned())
    }
    async fn update_password(&self, id: UserId, password_hash: &str) -> RepoResult<Option<i64>> {
        Ok(self
            .lock()
            .users
            .iter_mut()
            .find(|c| c.user.id == id)
            .map(|credentials| {
                credentials.password_hash = password_hash.to_string();
                credentials.session_version += 1;
                credentials.session_version
            }))
    }
    async fn create_first(
        &self,
        credentials: &Credentials,
        primary_member: &Member,
    ) -> RepoResult<bool> {
        let mut state = self.lock();
        if !state.users.is_empty() {
            return Ok(false);
        }
        state.users.push(credentials.clone());
        if !state
            .members
            .iter()
            .any(|member| member.name == primary_member.name)
        {
            let is_primary = !state.members.iter().any(|member| member.is_primary);
            state.members.push(Member {
                is_primary,
                ..primary_member.clone()
            });
        }
        Ok(true)
    }
}

#[async_trait]
impl PortfolioRepository for InMemoryStore {
    async fn load(&self) -> RepoResult<PortfolioData> {
        let state = self.lock();
        Ok(PortfolioData {
            members: sorted_members(&state.members),
            accounts: state.accounts.clone(),
            holdings: state.holdings.clone(),
        })
    }
    async fn members(&self) -> RepoResult<Vec<Member>> {
        Ok(sorted_members(&self.lock().members))
    }
    async fn insert_member(&self, member: &Member) -> RepoResult<()> {
        let mut state = self.lock();
        if state
            .members
            .iter()
            .any(|existing| existing.name == member.name)
        {
            return Err(RepoError::Conflict);
        }
        state.members.push(member.clone());
        Ok(())
    }
    async fn update_member(&self, member: &Member) -> RepoResult<()> {
        let mut state = self.lock();
        if state
            .members
            .iter()
            .any(|other| other.id != member.id && other.name == member.name)
        {
            return Err(RepoError::Conflict);
        }
        if let Some(existing) = state
            .members
            .iter_mut()
            .find(|existing| existing.id == member.id)
        {
            *existing = member.clone();
        }
        Ok(())
    }
    async fn delete_member(&self, id: MemberId) -> RepoResult<()> {
        let mut state = self.lock();
        if state
            .accounts
            .iter()
            .any(|account| account.owners.includes(id))
        {
            return Err(RepoError::Reference);
        }
        state.members.retain(|member| member.id != id);
        state.snapshots.retain(|(member, _)| *member != Some(id));
        for goal in &mut state.goals {
            goal.member_ids.retain(|member| *member != id);
        }
        Ok(())
    }
    async fn save_accounts(&self, writes: &[AccountWrite]) -> RepoResult<()> {
        let mut state = self.lock();
        writes.iter().for_each(|write| state.save_account(write));
        Ok(())
    }
    async fn delete_account(&self, id: AccountId) -> RepoResult<bool> {
        let mut state = self.lock();
        let before = state.accounts.len();
        state.accounts.retain(|account| account.id != id);
        state.holdings.retain(|holding| holding.account_id != id);
        state
            .transactions
            .retain(|transaction| transaction.account_id != id);
        for account in &mut state.accounts {
            if let Some(loan) = &mut account.loan
                && loan.property_account_id == Some(id)
            {
                loan.property_account_id = None;
            }
        }
        for goal in &mut state.goals {
            goal.account_ids.retain(|account| *account != id);
        }
        state.drop_orphan_operations();
        Ok(state.accounts.len() < before)
    }
    async fn save_holding(&self, holding: &Holding, at: DateTime<Utc>) -> RepoResult<()> {
        let mut state = self.lock();
        match state
            .holdings
            .iter_mut()
            .find(|existing| existing.id == holding.id)
        {
            Some(existing) => *existing = holding.clone(),
            None => state.holdings.push(holding.clone()),
        }
        state.touch(holding.account_id, at);
        Ok(())
    }
    async fn delete_holding(&self, id: HoldingId, at: DateTime<Utc>) -> RepoResult<bool> {
        let mut state = self.lock();
        let Some(index) = state.holdings.iter().position(|holding| holding.id == id) else {
            return Ok(false);
        };
        let removed = state.holdings.remove(index);
        state.drop_orphan_operations();
        state.touch(removed.account_id, at);
        Ok(true)
    }
    async fn update_prices(
        &self,
        prices: &[(HoldingId, f64)],
        at: DateTime<Utc>,
    ) -> RepoResult<()> {
        let mut state = self.lock();
        if state.fail_price_updates {
            return Err(RepoError::Backend("panne simulée".into()));
        }
        let date = at.date_naive();
        for (id, price) in prices {
            let Some(holding) = state.holdings.iter_mut().find(|holding| holding.id == *id) else {
                continue;
            };
            holding.current_price = *price;
            holding.price_source = PriceSource::Auto;
            holding.price_updated_at = Some(at);
            let point = PricePoint {
                date,
                price: *price,
                currency: holding.currency.clone(),
            };
            state
                .price_history
                .retain(|(holding, existing)| !(holding == id && existing.date == date));
            state.price_history.push((*id, point));
        }
        Ok(())
    }
    async fn price_history(
        &self,
        holding: HoldingId,
        from: NaiveDate,
    ) -> RepoResult<Vec<PricePoint>> {
        let mut points: Vec<PricePoint> = self
            .lock()
            .price_history
            .iter()
            .filter(|(id, point)| *id == holding && point.date >= from)
            .map(|(_, point)| point.clone())
            .collect();
        points.sort_by_key(|point| point.date);
        Ok(points)
    }
}

fn sorted_members(members: &[Member]) -> Vec<Member> {
    let mut sorted = members.to_vec();
    sorted.sort_by_key(|member| !member.is_primary);
    sorted
}

#[async_trait]
impl FxRateRepository for InMemoryStore {
    async fn rates(&self) -> RepoResult<(FxRates, Option<DateTime<Utc>>)> {
        let state = self.lock();
        Ok((
            FxRates::new(state.rates.iter().cloned()),
            state.rates_updated_at,
        ))
    }
    async fn save_rates(&self, rates: &FxRates, at: DateTime<Utc>) -> RepoResult<()> {
        let mut state = self.lock();
        for (currency, rate) in rates.iter() {
            state.rates.retain(|(existing, _)| existing != currency);
            state.rates.push((currency.clone(), rate));
            state.rates_updated_at = Some(at);
        }
        Ok(())
    }
}

#[async_trait]
impl TransactionRepository for InMemoryStore {
    async fn page(
        &self,
        account: AccountId,
        limit: i64,
        offset: i64,
    ) -> RepoResult<(Vec<Transaction>, u64)> {
        let state = self.lock();
        let mut items: Vec<Transaction> = state
            .transactions
            .iter()
            .filter(|t| t.account_id == account)
            .cloned()
            .collect();
        items.reverse();
        items.sort_by_key(|t| std::cmp::Reverse(t.date));
        let total = items.len() as u64;
        Ok((
            items
                .into_iter()
                .skip(offset as usize)
                .take(limit as usize)
                .collect(),
            total,
        ))
    }
    async fn import(&self, account: &Account, transactions: &[Transaction]) -> RepoResult<u64> {
        let mut state = self.lock();
        state.save_account(&AccountWrite::account_only(account.clone()));
        let mut inserted = 0;
        for transaction in transactions {
            if !state
                .transactions
                .iter()
                .any(|existing| existing.is_duplicate_of(transaction))
            {
                state.transactions.push(transaction.clone());
                inserted += 1;
            }
        }
        Ok(inserted)
    }
    async fn delete(&self, id: TransactionId) -> RepoResult<bool> {
        let mut state = self.lock();
        let before = state.transactions.len();
        state
            .transactions
            .retain(|transaction| transaction.id != id);
        Ok(state.transactions.len() < before)
    }
    async fn between(&self, from: NaiveDate, to: NaiveDate) -> RepoResult<Vec<Transaction>> {
        Ok(self
            .lock()
            .transactions
            .iter()
            .filter(|t| t.date >= from && t.date <= to)
            .cloned()
            .collect())
    }
    async fn by_id(&self, id: TransactionId) -> RepoResult<Option<Transaction>> {
        Ok(self
            .lock()
            .transactions
            .iter()
            .find(|t| t.id == id)
            .cloned())
    }
    async fn annotate(
        &self,
        transaction: &Transaction,
        rule: Option<&CategoryRule>,
    ) -> RepoResult<()> {
        let mut state = self.lock();
        if let Some(existing) = state
            .transactions
            .iter_mut()
            .find(|t| t.id == transaction.id)
        {
            existing.manual_category = transaction.manual_category.clone();
            existing.transfer_override = transaction.transfer_override;
        }
        if let Some(rule) = rule {
            match state
                .rules
                .iter_mut()
                .find(|r| r.pattern() == rule.pattern())
            {
                Some(existing) => {
                    *existing = CategoryRule::new(existing.id, existing.pattern(), rule.category())
                        .map_err(|e| RepoError::Backend(e.to_string()))?;
                }
                None => state.rules.push(rule.clone()),
            }
        }
        Ok(())
    }
    async fn categories(&self) -> RepoResult<Vec<String>> {
        let state = self.lock();
        let mut categories: Vec<String> = state
            .transactions
            .iter()
            .flat_map(|t| [t.category.clone(), t.manual_category.clone()])
            .flatten()
            .collect();
        categories.sort();
        categories.dedup();
        Ok(categories)
    }
}

#[async_trait]
impl BudgetRepository for InMemoryStore {
    async fn rules(&self) -> RepoResult<Vec<CategoryRule>> {
        Ok(self.lock().rules.clone())
    }
    async fn delete_rule(&self, id: BudgetRuleId) -> RepoResult<bool> {
        let mut state = self.lock();
        let before = state.rules.len();
        state.rules.retain(|rule| rule.id != id);
        Ok(state.rules.len() < before)
    }
    async fn targets(&self) -> RepoResult<Vec<BudgetTarget>> {
        Ok(self.lock().targets.clone())
    }
    async fn save_target(&self, target: &BudgetTarget) -> RepoResult<()> {
        let mut state = self.lock();
        state.targets.retain(|t| t.category() != target.category());
        state.targets.push(target.clone());
        Ok(())
    }
    async fn delete_target(&self, category: &str) -> RepoResult<bool> {
        let mut state = self.lock();
        let before = state.targets.len();
        state.targets.retain(|t| t.category() != category);
        Ok(state.targets.len() < before)
    }
}

#[async_trait]
impl OperationRepository for InMemoryStore {
    async fn operations(&self, account: AccountId) -> RepoResult<Vec<Operation>> {
        Ok(self
            .lock()
            .operations
            .iter()
            .filter(|operation| operation.account_id == account)
            .cloned()
            .collect())
    }
    async fn find_operation(&self, id: OperationId) -> RepoResult<Option<Operation>> {
        Ok(self
            .lock()
            .operations
            .iter()
            .find(|operation| operation.id == id)
            .cloned())
    }
    async fn apply_operation(
        &self,
        change: &OperationChange,
        positions: &[PositionUpdate],
        at: DateTime<Utc>,
    ) -> RepoResult<()> {
        let mut state = self.lock();
        let account = match change {
            OperationChange::Save(operation) => {
                state
                    .operations
                    .retain(|existing| existing.id != operation.id);
                state.operations.push(operation.clone());
                operation.account_id
            }
            OperationChange::Delete { id, account } => {
                state.operations.retain(|existing| existing.id != *id);
                *account
            }
        };
        for update in positions {
            if let Some(holding) = state.holdings.iter_mut().find(|h| h.id == update.holding) {
                holding.quantity = update.quantity;
                holding.buy_price = update.buy_price;
            }
        }
        state.touch(account, at);
        Ok(())
    }
}

#[async_trait]
impl SnapshotRepository for InMemoryStore {
    async fn upsert(
        &self,
        date: NaiveDate,
        member: Option<MemberId>,
        snapshot: &Snapshot,
    ) -> RepoResult<()> {
        let mut state = self.lock();
        state
            .snapshots
            .retain(|(m, point)| !(point.date == date && *m == member));
        state.snapshots.push((
            member,
            SnapshotPoint {
                date,
                totals: snapshot.totals,
                accounts: Some(snapshot.accounts.clone()),
                corrected: false,
            },
        ));
        Ok(())
    }
    async fn since(
        &self,
        member: Option<MemberId>,
        from: NaiveDate,
    ) -> RepoResult<Vec<SnapshotPoint>> {
        let mut points: Vec<SnapshotPoint> = self
            .lock()
            .snapshots
            .iter()
            .filter(|(m, point)| *m == member && point.date >= from)
            .map(|(_, point)| point.clone())
            .collect();
        points.sort_by_key(|point| point.date);
        Ok(points)
    }
    async fn correct(
        &self,
        date: NaiveDate,
        member: Option<MemberId>,
        totals: Totals,
    ) -> RepoResult<bool> {
        let mut state = self.lock();
        let found = state
            .snapshots
            .iter_mut()
            .find(|(m, point)| *m == member && point.date == date);
        Ok(found
            .map(|(_, point)| {
                point.totals = totals;
                point.accounts = None;
                point.corrected = true;
            })
            .is_some())
    }
    async fn delete_range(&self, range: DateRange) -> RepoResult<u64> {
        let mut state = self.lock();
        let mut days: Vec<NaiveDate> = state
            .snapshots
            .iter()
            .map(|(_, point)| point.date)
            .filter(|date| range.contains(*date))
            .collect();
        days.sort();
        days.dedup();
        state
            .snapshots
            .retain(|(_, point)| !range.contains(point.date));
        Ok(days.len() as u64)
    }
    async fn delete_all(&self) -> RepoResult<u64> {
        let mut state = self.lock();
        let mut days: Vec<NaiveDate> = state.snapshots.iter().map(|(_, p)| p.date).collect();
        days.sort();
        days.dedup();
        state.snapshots.clear();
        Ok(days.len() as u64)
    }
}

#[async_trait]
impl BenchmarkRepository for InMemoryStore {
    async fn benchmark(&self) -> RepoResult<Option<Benchmark>> {
        Ok(self.lock().benchmark.clone())
    }
    async fn save_benchmark(&self, benchmark: &Benchmark, reset_prices: bool) -> RepoResult<()> {
        let mut state = self.lock();
        if reset_prices {
            state.benchmark_prices.clear();
        }
        state.benchmark = Some(benchmark.clone());
        Ok(())
    }
    async fn delete_benchmark(&self) -> RepoResult<bool> {
        let mut state = self.lock();
        state.benchmark_prices.clear();
        Ok(state.benchmark.take().is_some())
    }
    async fn record_benchmark_price(&self, date: NaiveDate, price: f64) -> RepoResult<()> {
        let mut state = self.lock();
        state.benchmark_prices.retain(|(day, _)| *day != date);
        state.benchmark_prices.push((date, price));
        Ok(())
    }
    async fn benchmark_prices(&self, from: NaiveDate) -> RepoResult<Vec<PricePoint>> {
        let state = self.lock();
        let Some(benchmark) = &state.benchmark else {
            return Ok(Vec::new());
        };
        let mut prices: Vec<PricePoint> = state
            .benchmark_prices
            .iter()
            .filter(|(date, _)| *date >= from)
            .map(|(date, price)| PricePoint {
                date: *date,
                price: *price,
                currency: benchmark.currency().clone(),
            })
            .collect();
        prices.sort_by_key(|point| point.date);
        Ok(prices)
    }
}

#[async_trait]
impl GoalRepository for InMemoryStore {
    async fn list(&self) -> RepoResult<Vec<Goal>> {
        Ok(self.lock().goals.clone())
    }
    async fn save(&self, goal: &Goal) -> RepoResult<()> {
        let mut state = self.lock();
        match state
            .goals
            .iter_mut()
            .find(|existing| existing.id == goal.id)
        {
            Some(existing) => *existing = goal.clone(),
            None => state.goals.push(goal.clone()),
        }
        Ok(())
    }
    async fn delete(&self, id: GoalId) -> RepoResult<bool> {
        let mut state = self.lock();
        let before = state.goals.len();
        state.goals.retain(|goal| goal.id != id);
        Ok(state.goals.len() < before)
    }
}

#[async_trait]
impl ConnectionRepository for InMemoryStore {
    async fn list(&self) -> RepoResult<Vec<Connection>> {
        Ok(self.lock().connections.clone())
    }
    async fn find(&self, id: ConnectionId) -> RepoResult<Option<Connection>> {
        Ok(self
            .lock()
            .connections
            .iter()
            .find(|connection| connection.id == id)
            .cloned())
    }
    async fn save(&self, connection: &Connection) -> RepoResult<()> {
        let mut state = self.lock();
        match state
            .connections
            .iter_mut()
            .find(|existing| existing.id == connection.id)
        {
            Some(existing) => *existing = connection.clone(),
            None => state.connections.push(connection.clone()),
        }
        Ok(())
    }
    async fn delete(&self, id: ConnectionId) -> RepoResult<bool> {
        let mut state = self.lock();
        let before = state.connections.len();
        state.connections.retain(|connection| connection.id != id);
        Ok(state.connections.len() < before)
    }
}

#[async_trait]
impl AlertRepository for InMemoryStore {
    async fn alerts(&self) -> RepoResult<Vec<Alert>> {
        Ok(self.lock().alerts.clone())
    }
    async fn find_alert(&self, id: AlertId) -> RepoResult<Option<Alert>> {
        Ok(self.lock().alerts.iter().find(|a| a.id == id).cloned())
    }
    async fn save_alerts(&self, alerts: &[Alert]) -> RepoResult<()> {
        let mut state = self.lock();
        for alert in alerts {
            if state
                .alerts
                .iter()
                .any(|existing| existing.key == alert.key && existing.id != alert.id)
            {
                return Err(RepoError::Conflict);
            }
            match state
                .alerts
                .iter_mut()
                .find(|existing| existing.id == alert.id)
            {
                Some(existing) => *existing = alert.clone(),
                None => state.alerts.push(alert.clone()),
            }
        }
        Ok(())
    }
    async fn rules(&self) -> RepoResult<Vec<AlertRule>> {
        Ok(self.lock().alert_rules.clone())
    }
    async fn save_rule(&self, rule: &AlertRule) -> RepoResult<()> {
        let mut state = self.lock();
        state
            .alert_rules
            .retain(|existing| existing.kind() != rule.kind());
        state.alert_rules.push(rule.clone());
        Ok(())
    }
}

#[async_trait]
impl ImportRepository for InMemoryStore {
    async fn apply(&self, batch: &ImportBatch) -> RepoResult<()> {
        let mut state = self.lock();
        for member in &batch.members {
            match state
                .members
                .iter_mut()
                .find(|existing| existing.id == member.id)
            {
                Some(existing) => *existing = member.clone(),
                None => state.members.push(member.clone()),
            }
        }
        batch
            .accounts
            .iter()
            .for_each(|write| state.save_account(write));
        for goal in &batch.goals {
            match state
                .goals
                .iter_mut()
                .find(|existing| existing.id == goal.id)
            {
                Some(existing) => *existing = goal.clone(),
                None => state.goals.push(goal.clone()),
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- temps, sécurité, services externes

pub struct FixedClock(Mutex<DateTime<Utc>>);

impl FixedClock {
    /// Date fixe des tests : 1er janvier 2026 à midi (UTC).
    pub fn new() -> Self {
        Self(Mutex::new(
            Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap(),
        ))
    }

    pub fn advance_days(&self, days: i64) {
        *self.0.lock().unwrap() += Duration::days(days);
    }

    pub fn advance_seconds(&self, seconds: i64) {
        *self.0.lock().unwrap() += Duration::seconds(seconds);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

#[derive(Default)]
pub struct FakeHasher(AtomicUsize);

impl FakeHasher {
    pub fn verifications(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl PasswordHasher for FakeHasher {
    async fn hash(&self, password: &str) -> Result<String, ProviderError> {
        Ok(format!("hash({password})"))
    }
    async fn verify(&self, password: &str, hash: &str) -> bool {
        self.0.fetch_add(1, Ordering::SeqCst);
        hash == format!("hash({password})")
    }
}

/// Jetons lisibles « jeton|<utilisateur>|<version de session>|<expiration> », valables 30 jours.
pub struct FakeTokens;

impl TokenService for FakeTokens {
    fn issue(&self, subject: TokenSubject, now: DateTime<Utc>) -> Result<String, ProviderError> {
        Ok(format!(
            "jeton|{}|{}|{}",
            subject.user,
            subject.session_version,
            (now + Duration::days(30)).timestamp()
        ))
    }
    fn verify(&self, token: &str, now: DateTime<Utc>) -> Option<TokenSubject> {
        let mut parts = token.split('|').skip(1);
        let user = Uuid::parse_str(parts.next()?).ok()?;
        let session_version = parts.next()?.parse().ok()?;
        let expires: i64 = parts.next()?.parse().ok()?;
        (expires > now.timestamp()).then(|| TokenSubject {
            user: UserId::from_uuid(user),
            session_version,
        })
    }
}

pub struct FakeFx(Mutex<Result<Vec<(String, f64)>, String>>);

impl FakeFx {
    pub fn set(&self, rates: &[(&str, f64)]) {
        *self.0.lock().unwrap() = Ok(rates
            .iter()
            .map(|(code, rate)| (code.to_string(), *rate))
            .collect());
    }
    pub fn fail(&self, reason: &str) {
        *self.0.lock().unwrap() = Err(reason.to_string());
    }
}

#[async_trait]
impl FxProvider for FakeFx {
    async fn eur_rates(&self) -> Result<Vec<(String, f64)>, ProviderError> {
        self.0.lock().unwrap().clone().map_err(ProviderError)
    }
}

/// Fournisseur de cours indexé par symbole, pour un type de position. Retient les symboles demandés.
pub struct FakePrices {
    kind: HoldingKind,
    quotes: Mutex<HashMap<String, (f64, String)>>,
    requested: Mutex<Vec<String>>,
}

impl FakePrices {
    pub fn new(kind: HoldingKind) -> Self {
        Self {
            kind,
            quotes: Mutex::new(HashMap::new()),
            requested: Mutex::new(Vec::new()),
        }
    }
    /// Symboles demandés à ce fournisseur, dans l'ordre.
    pub fn requested(&self) -> Vec<String> {
        self.requested.lock().unwrap().clone()
    }
    pub fn set(&self, symbol: &str, price: f64, currency: &str) {
        self.quotes
            .lock()
            .unwrap()
            .insert(symbol.to_string(), (price, currency.to_string()));
    }
}

#[async_trait]
impl PriceProvider for FakePrices {
    fn handles(&self, kind: HoldingKind) -> bool {
        kind == self.kind
    }
    async fn quotes(
        &self,
        requests: &[QuoteRequest],
    ) -> HashMap<HoldingId, Result<Quote, ProviderError>> {
        self.requested
            .lock()
            .unwrap()
            .extend(requests.iter().filter_map(|r| r.symbol.clone()));
        let quotes = self.quotes.lock().unwrap();
        requests
            .iter()
            .map(|request| {
                let found = request
                    .symbol
                    .as_ref()
                    .and_then(|symbol| quotes.get(symbol));
                let outcome = match found {
                    Some((price, currency)) => Ok(Quote {
                        price: *price,
                        currency: Currency::parse(currency).unwrap(),
                    }),
                    None => Err(ProviderError("cours introuvable".into())),
                };
                (request.holding_id, outcome)
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct FakeBank {
    configured: bool,
    accounts: Vec<BankAccount>,
    failure: Arc<Mutex<Option<String>>>,
}

impl FakeBank {
    pub fn new(configured: bool, accounts: Vec<BankAccount>) -> Self {
        Self {
            configured,
            accounts,
            failure: Arc::default(),
        }
    }
    pub fn fail_next_fetch(&self, reason: &str) {
        *self.failure.lock().unwrap() = Some(reason.to_string());
    }
}

#[async_trait]
impl BankProvider for FakeBank {
    fn id(&self) -> &str {
        "demo"
    }
    fn name(&self) -> &str {
        "Banque Démo"
    }
    fn configured(&self) -> bool {
        self.configured
    }
    async fn create_connection(&self) -> Result<NewBankConnection, ProviderError> {
        Ok(NewBankConnection {
            external_id: "connexion-démo".into(),
            redirect_url: "https://banque.example/autoriser".into(),
            institution: Some("Banque Démo".into()),
        })
    }
    async fn fetch_accounts(
        &self,
        _connection_external_id: &str,
    ) -> Result<Vec<BankAccount>, ProviderError> {
        match self.failure.lock().unwrap().take() {
            Some(reason) => Err(ProviderError(reason)),
            None => Ok(self.accounts.clone()),
        }
    }
}

// ---------------------------------------------------------------- application de test et fixtures

pub struct TestApp {
    pub app: App,
    pub store: Arc<InMemoryStore>,
    pub clock: Arc<FixedClock>,
    pub hasher: Arc<FakeHasher>,
    pub fx: Arc<FakeFx>,
    pub securities: Arc<FakePrices>,
    /// Fournisseur de repli pour les titres, essayé après `securities`.
    pub fallback: Arc<FakePrices>,
    pub crypto: Arc<FakePrices>,
    pub alice: MemberId,
    pub bob: MemberId,
}

impl TestApp {
    async fn build(with_household: bool, bank: Option<FakeBank>, tokens: Settings) -> Self {
        let store = Arc::new(InMemoryStore::default());
        let clock = Arc::new(FixedClock::new());
        let hasher = Arc::new(FakeHasher::default());
        let fx = Arc::new(FakeFx(Mutex::new(Ok(Vec::new()))));
        let securities = Arc::new(FakePrices::new(HoldingKind::Security));
        let fallback = Arc::new(FakePrices::new(HoldingKind::Security));
        let crypto = Arc::new(FakePrices::new(HoldingKind::Crypto));
        let (alice, bob) = (MemberId::generate(), MemberId::generate());
        if with_household {
            let mut state = store.0.lock().unwrap();
            state.members.push(Member {
                id: alice,
                name: "Alice".into(),
                color: "#6366f1".into(),
                is_primary: true,
            });
            state.members.push(Member {
                id: bob,
                name: "Bob".into(),
                color: "#ec4899".into(),
                is_primary: false,
            });
        }
        let ports = Ports {
            users: store.clone(),
            login_journal: store.clone(),
            portfolio: store.clone(),
            fx_rates: store.clone(),
            transactions: store.clone(),
            budget: store.clone(),
            operations: store.clone(),
            snapshots: store.clone(),
            benchmarks: store.clone(),
            goals: store.clone(),
            connections: store.clone(),
            alerts: store.clone(),
            imports: store.clone(),
            clock: clock.clone(),
            hasher: hasher.clone(),
            tokens: Arc::new(FakeTokens),
            fx_provider: fx.clone(),
            price_providers: vec![crypto.clone(), securities.clone(), fallback.clone()],
            bank_providers: bank
                .into_iter()
                .map(|b| Arc::new(b) as Arc<dyn BankProvider>)
                .collect(),
        };
        let app = App::build(ports, tokens).await.unwrap();
        Self {
            app,
            store,
            clock,
            hasher,
            fx,
            securities,
            fallback,
            crypto,
            alice,
            bob,
        }
    }

    /// Application sans aucune donnée (avant la première configuration).
    pub async fn empty() -> Self {
        Self::build(false, None, Settings::default()).await
    }

    /// Application vide dont la première configuration exige un code.
    pub async fn with_setup_token(token: &str) -> Self {
        let settings = Settings {
            setup_token: Some(token.to_string()),
            ..Settings::default()
        };
        Self::build(false, None, settings).await
    }

    /// Application vide où un mot de passe perdu se réinitialise avec ce code de secours.
    pub async fn with_recovery_token(token: &str) -> Self {
        let settings = Settings {
            recovery_token: Some(token.to_string()),
            ..Settings::default()
        };
        Self::build(false, None, settings).await
    }

    /// Foyer fictif dont la tâche quotidienne exige ce secret.
    pub async fn with_cron_secret(secret: &str) -> Self {
        let settings = Settings {
            cron_secret: Some(secret.to_string()),
            ..Settings::default()
        };
        Self::build(true, None, settings).await
    }

    /// Foyer fictif : Alice (membre principal) et Bob.
    pub async fn household() -> Self {
        Self::build(true, None, Settings::default()).await
    }

    pub async fn with_bank(bank: FakeBank) -> Self {
        Self::build(true, Some(bank), Settings::default()).await
    }

    pub fn set_rate(&self, code: &str, rate: f64) {
        self.store
            .0
            .lock()
            .unwrap()
            .rates
            .push((Currency::parse(code).unwrap(), rate));
    }

    pub async fn create(&self, input: AccountInput) -> AccountId {
        self.app
            .accounts
            .create(&input)
            .await
            .unwrap()
            .view
            .account
            .id
    }

    pub async fn checking(&self, shares: &[(MemberId, f64)], cash: f64) -> AccountId {
        self.create(AccountInput {
            name: Some("Compte courant".into()),
            kind: Some("checking".into()),
            cash_balance: Some(cash),
            owners: Some(owners(shares)),
            ..Default::default()
        })
        .await
    }

    pub async fn investment(&self, owner: MemberId, cash: f64) -> AccountId {
        self.create(AccountInput {
            name: Some("PEA".into()),
            kind: Some("investment".into()),
            envelope: Some(Some("pea".into())),
            cash_balance: Some(cash),
            owners: Some(owners(&[(owner, 1.0)])),
            ..Default::default()
        })
        .await
    }

    /// Maison à 300 k€ louée 1 200 €/mois et prêt de 200 k€ qui la finance, tous deux à 50/50.
    /// Le prêt démarre le 1er juin 2026, après la date fixe des tests.
    pub async fn house_with_loan(&self) -> (AccountId, AccountId) {
        let shares = owners(&[(self.alice, 0.5), (self.bob, 0.5)]);
        let house = self
            .create(AccountInput {
                name: Some("Maison".into()),
                kind: Some("real_estate".into()),
                owners: Some(shares.clone()),
                property: Some(property_input()),
                ..Default::default()
            })
            .await;
        let loan = self
            .create(AccountInput {
                name: Some("Prêt maison".into()),
                kind: Some("loan".into()),
                owners: Some(shares),
                loan: Some(loan_input(Some(house))),
                ..Default::default()
            })
            .await;
        (house, loan)
    }
}

pub fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

pub fn owners(shares: &[(MemberId, f64)]) -> Vec<OwnerInput> {
    shares
        .iter()
        .map(|(member, share)| OwnerInput {
            member_id: member.as_uuid(),
            share: *share,
        })
        .collect()
}

pub fn property_input() -> PropertyInput {
    PropertyInput {
        address: Some(Some("1 rue des Exemples".into())),
        category: Some("rental".into()),
        building_type: Some("house".into()),
        surface_m2: Some(Some(90.0)),
        purchase_price: Some(250_000.0),
        purchase_fees: Some(20_000.0),
        purchased_at: Some(Some(date(2022, 3, 1))),
        current_value: Some(300_000.0),
        monthly_rent: Some(1200.0),
        monthly_charges: Some(100.0),
        yearly_property_tax: Some(1200.0),
    }
}

pub fn loan_input(property_account: Option<AccountId>) -> LoanInput {
    LoanInput {
        property_account_id: property_account.map(|id| Some(id.as_uuid())),
        principal: Some(200_000.0),
        interest_rate_pct: Some(4.0),
        insurance_rate_pct: Some(0.3),
        duration_months: Some(240),
        start_date: Some(date(2026, 6, 1)),
    }
}

pub fn holding_input(
    name: &str,
    quantity: f64,
    buy_price: Option<f64>,
    current_price: f64,
) -> HoldingInput {
    HoldingInput {
        name: Some(name.into()),
        quantity: Some(quantity),
        buy_price: Some(buy_price),
        current_price: Some(current_price),
        ..Default::default()
    }
}
