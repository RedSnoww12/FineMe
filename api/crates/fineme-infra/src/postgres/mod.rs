//! Adaptateur Postgres : implémente tous les ports de persistance avec sqlx (requêtes à l'exécution).

mod rows;

use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use fineme_application::ports::{
    AccountWrite, BudgetRepository, ConnectionRepository, Credentials, FxRateRepository,
    GoalRepository, ImportBatch, ImportRepository, LoginJournal, OperationChange,
    OperationRepository, PortfolioData, PortfolioRepository, PositionUpdate, PricePoint, RepoError,
    RepoResult, SnapshotPoint, SnapshotRepository, TransactionRepository, UserRepository,
};
use fineme_domain::account::Account;
use fineme_domain::budget::{BudgetTarget, CategoryRule};
use fineme_domain::connection::Connection;
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::goal::Goal;
use fineme_domain::history::DateRange;
use fineme_domain::holding::Holding;
use fineme_domain::ids::{
    AccountId, BudgetRuleId, ConnectionId, GoalId, HoldingId, MemberId, OperationId, TransactionId,
    UserId,
};
use fineme_domain::loan::Loan;
use fineme_domain::login::{LoginAction, LoginEvent, LoginOrigin};
use fineme_domain::member::{Member, User};
use fineme_domain::operation::Operation;
use fineme_domain::portfolio::{AccountSnapshot, Snapshot, Totals};
use fineme_domain::property::Property;
use fineme_domain::transaction::Transaction;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use rows::{
    AccountRow, BudgetRuleRow, BudgetTargetRow, ConnectionRow, GoalRow, HoldingRow, LoanRow,
    MemberRow, OperationRow, OwnerRow, PropertyRow, SnapshotAccountRow, TransactionRow,
};

#[derive(Debug, thiserror::Error)]
pub enum PostgresError {
    #[error("DATABASE_URL invalide : {0}")]
    InvalidUrl(#[source] sqlx::Error),
    #[error("connexion à la base impossible : {0}")]
    Connect(#[source] sqlx::Error),
    #[error("échec des migrations : {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
}

/// Ouvre le pool de connexions. Le cache de requêtes préparées est désactivé pour rester
/// compatible avec les poolers (pgbouncer / Supabase).
pub async fn connect(database_url: &str) -> Result<PgPool, PostgresError> {
    let options = PgConnectOptions::from_str(database_url)
        .map_err(PostgresError::InvalidUrl)?
        .statement_cache_capacity(0);
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(15))
        .connect_with(options)
        .await
        .map_err(PostgresError::Connect)
}

/// Applique les migrations embarquées (idempotent).
pub async fn migrate(pool: &PgPool) -> Result<(), PostgresError> {
    Ok(sqlx::migrate!("./migrations").run(pool).await?)
}

fn db(error: sqlx::Error) -> RepoError {
    if let sqlx::Error::Database(database) = &error {
        if database.is_unique_violation() {
            return RepoError::Conflict;
        }
        if database.is_foreign_key_violation() {
            return RepoError::Reference;
        }
    }
    RepoError::Backend(error.to_string())
}

/// Dépôt Postgres unique, implémentant chaque port de persistance.
#[derive(Clone)]
pub struct PgStore {
    pool: PgPool,
}

impl PgStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

// ---------------------------------------------------------------- écritures partagées (dans une transaction)

async fn upsert_member(conn: &mut PgConnection, member: &Member) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO members (id, name, color, is_primary) VALUES ($1, $2, $3, $4) \
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, color = EXCLUDED.color, is_primary = EXCLUDED.is_primary",
    )
    .bind(member.id.as_uuid())
    .bind(&member.name)
    .bind(&member.color)
    .bind(member.is_primary)
    .execute(conn)
    .await
    .map_err(db)?;
    Ok(())
}

async fn upsert_property(
    conn: &mut PgConnection,
    account_id: Uuid,
    p: &Property,
) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO properties (id, account_id, address, category, building_type, surface_m2, purchase_price, \
         purchase_fees, purchased_at, current_value, monthly_rent, monthly_charges, yearly_property_tax) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
         ON CONFLICT (account_id) DO UPDATE SET address = EXCLUDED.address, category = EXCLUDED.category, \
         building_type = EXCLUDED.building_type, surface_m2 = EXCLUDED.surface_m2, \
         purchase_price = EXCLUDED.purchase_price, purchase_fees = EXCLUDED.purchase_fees, \
         purchased_at = EXCLUDED.purchased_at, current_value = EXCLUDED.current_value, \
         monthly_rent = EXCLUDED.monthly_rent, monthly_charges = EXCLUDED.monthly_charges, \
         yearly_property_tax = EXCLUDED.yearly_property_tax",
    )
    .bind(p.id.as_uuid())
    .bind(account_id)
    .bind(&p.address)
    .bind(p.category.as_str())
    .bind(p.building_type.as_str())
    .bind(p.surface_m2)
    .bind(p.purchase_price)
    .bind(p.purchase_fees)
    .bind(p.purchased_at)
    .bind(p.current_value)
    .bind(p.monthly_rent)
    .bind(p.monthly_charges)
    .bind(p.yearly_property_tax)
    .execute(conn)
    .await
    .map_err(db)?;
    Ok(())
}

async fn upsert_loan(conn: &mut PgConnection, account_id: Uuid, loan: &Loan) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO loans (id, account_id, property_account_id, principal, interest_rate_pct, insurance_rate_pct, \
         duration_months, start_date) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         ON CONFLICT (account_id) DO UPDATE SET property_account_id = EXCLUDED.property_account_id, \
         principal = EXCLUDED.principal, interest_rate_pct = EXCLUDED.interest_rate_pct, \
         insurance_rate_pct = EXCLUDED.insurance_rate_pct, duration_months = EXCLUDED.duration_months, \
         start_date = EXCLUDED.start_date",
    )
    .bind(loan.id.as_uuid())
    .bind(account_id)
    .bind(loan.property_account_id.map(|id| id.as_uuid()))
    .bind(loan.terms.principal())
    .bind(loan.terms.interest_rate().value())
    .bind(loan.terms.insurance_rate().value())
    .bind(i32::try_from(loan.terms.duration_months()).unwrap_or(i32::MAX))
    .bind(loan.terms.start_date())
    .execute(conn)
    .await
    .map_err(db)?;
    Ok(())
}

async fn upsert_holding(conn: &mut PgConnection, h: &Holding) -> RepoResult<()> {
    sqlx::query(
        "INSERT INTO holdings (id, account_id, kind, name, isin, symbol, security_type, quantity, buy_price, \
         current_price, currency, expense_ratio, price_source, price_updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
         ON CONFLICT (id) DO UPDATE SET kind = EXCLUDED.kind, name = EXCLUDED.name, isin = EXCLUDED.isin, \
         symbol = EXCLUDED.symbol, security_type = EXCLUDED.security_type, quantity = EXCLUDED.quantity, \
         buy_price = EXCLUDED.buy_price, current_price = EXCLUDED.current_price, currency = EXCLUDED.currency, \
         expense_ratio = EXCLUDED.expense_ratio, price_source = EXCLUDED.price_source, \
         price_updated_at = EXCLUDED.price_updated_at",
    )
    .bind(h.id.as_uuid())
    .bind(h.account_id.as_uuid())
    .bind(h.kind.as_str())
    .bind(&h.name)
    .bind(&h.isin)
    .bind(&h.symbol)
    .bind(&h.security_type)
    .bind(h.quantity)
    .bind(h.buy_price)
    .bind(h.current_price)
    .bind(h.currency.as_str())
    .bind(h.expense_ratio)
    .bind(h.price_source.as_str())
    .bind(h.price_updated_at)
    .execute(conn)
    .await
    .map_err(db)?;
    Ok(())
}

async fn touch_account(
    conn: &mut PgConnection,
    account_id: Uuid,
    at: DateTime<Utc>,
) -> RepoResult<()> {
    sqlx::query("UPDATE accounts SET updated_at = $2 WHERE id = $1")
        .bind(account_id)
        .bind(at)
        .execute(conn)
        .await
        .map_err(db)?;
    Ok(())
}

/// Écrit l'agrégat compte : ligne, détenteurs, bien, prêt et, si fournies, l'ensemble de ses positions.
async fn save_account(conn: &mut PgConnection, write: &AccountWrite) -> RepoResult<()> {
    let a = &write.account;
    let id = a.id.as_uuid();
    sqlx::query(
        "INSERT INTO accounts (id, name, institution, kind, envelope, currency, cash_balance, annual_yield_pct, \
         source, external_id, connection_id, opened_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, institution = EXCLUDED.institution, \
         kind = EXCLUDED.kind, envelope = EXCLUDED.envelope, currency = EXCLUDED.currency, \
         cash_balance = EXCLUDED.cash_balance, annual_yield_pct = EXCLUDED.annual_yield_pct, \
         source = EXCLUDED.source, external_id = EXCLUDED.external_id, connection_id = EXCLUDED.connection_id, \
         opened_at = EXCLUDED.opened_at, updated_at = EXCLUDED.updated_at",
    )
    .bind(id)
    .bind(&a.name)
    .bind(&a.institution)
    .bind(a.kind.as_str())
    .bind(&a.envelope)
    .bind(a.currency.as_str())
    .bind(a.cash_balance)
    .bind(a.annual_yield_pct)
    .bind(a.source.as_str())
    .bind(&a.external_id)
    .bind(a.connection_id.map(|c| c.as_uuid()))
    .bind(a.opened_at)
    .bind(a.updated_at)
    .execute(&mut *conn)
    .await
    .map_err(db)?;

    sqlx::query("DELETE FROM account_owners WHERE account_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    for (member, share) in a.owners.owners() {
        sqlx::query(
            "INSERT INTO account_owners (account_id, member_id, share) VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(member.as_uuid())
        .bind(share.value())
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    }

    match &a.property {
        Some(property) => upsert_property(conn, id, property).await?,
        None => {
            sqlx::query("DELETE FROM properties WHERE account_id = $1")
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(db)?;
        }
    }
    match &a.loan {
        Some(loan) => upsert_loan(conn, id, loan).await?,
        None => {
            sqlx::query("DELETE FROM loans WHERE account_id = $1")
                .bind(id)
                .execute(&mut *conn)
                .await
                .map_err(db)?;
        }
    }

    if let Some(holdings) = &write.holdings {
        // Les positions conservées gardent leur ligne (et leur date de création) ; les autres disparaissent.
        let kept: Vec<Uuid> = holdings
            .iter()
            .map(|holding| holding.id.as_uuid())
            .collect();
        sqlx::query("DELETE FROM holdings WHERE account_id = $1 AND id <> ALL($2)")
            .bind(id)
            .bind(&kept)
            .execute(&mut *conn)
            .await
            .map_err(db)?;
        for holding in holdings {
            upsert_holding(conn, holding).await?;
        }
    }
    Ok(())
}

async fn save_goal(conn: &mut PgConnection, g: &Goal) -> RepoResult<()> {
    let id = g.id.as_uuid();
    sqlx::query(
        "INSERT INTO goals (id, name, icon, kind, target_amount, target_date, monthly_contribution, \
         expected_return_pct, monthly_expenses, coverage_months) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
         ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name, icon = EXCLUDED.icon, kind = EXCLUDED.kind, \
         target_amount = EXCLUDED.target_amount, target_date = EXCLUDED.target_date, \
         monthly_contribution = EXCLUDED.monthly_contribution, expected_return_pct = EXCLUDED.expected_return_pct, \
         monthly_expenses = EXCLUDED.monthly_expenses, coverage_months = EXCLUDED.coverage_months",
    )
    .bind(id)
    .bind(&g.name)
    .bind(&g.icon)
    .bind(g.kind.as_str())
    .bind(g.target_amount)
    .bind(g.target_date)
    .bind(g.monthly_contribution)
    .bind(g.expected_return_pct)
    .bind(g.monthly_expenses)
    .bind(g.coverage_months)
    .execute(&mut *conn)
    .await
    .map_err(db)?;
    let accounts: Vec<Uuid> = g.account_ids.iter().map(|a| a.as_uuid()).collect();
    let members: Vec<Uuid> = g.member_ids.iter().map(|m| m.as_uuid()).collect();
    sqlx::query("DELETE FROM goal_accounts WHERE goal_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    sqlx::query("INSERT INTO goal_accounts (goal_id, account_id) SELECT $1, unnest($2::uuid[])")
        .bind(id)
        .bind(&accounts)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    sqlx::query("DELETE FROM goal_members WHERE goal_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    sqlx::query("INSERT INTO goal_members (goal_id, member_id) SELECT $1, unnest($2::uuid[])")
        .bind(id)
        .bind(&members)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    Ok(())
}

// ---------------------------------------------------------------- utilisateurs

type CredentialsRow = (Uuid, String, String, String, i64);

fn credentials((id, email, name, password_hash, session_version): CredentialsRow) -> Credentials {
    Credentials {
        user: User {
            id: UserId::from_uuid(id),
            email,
            name,
        },
        password_hash,
        session_version,
    }
}

#[async_trait]
impl UserRepository for PgStore {
    async fn exists_any(&self) -> RepoResult<bool> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users)")
            .fetch_one(&self.pool)
            .await
            .map_err(db)
    }

    async fn find_by_email(&self, email: &str) -> RepoResult<Option<Credentials>> {
        let row: Option<CredentialsRow> = sqlx::query_as(
            "SELECT id, email, name, password_hash, session_version FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(row.map(credentials))
    }

    async fn find_by_id(&self, id: UserId) -> RepoResult<Option<Credentials>> {
        let row: Option<CredentialsRow> = sqlx::query_as(
            "SELECT id, email, name, password_hash, session_version FROM users WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(row.map(credentials))
    }

    async fn update_password(&self, id: UserId, password_hash: &str) -> RepoResult<Option<i64>> {
        // Une seule requête : le hash et la version changent ensemble ou pas du tout.
        sqlx::query_scalar(
            "UPDATE users SET password_hash = $2, session_version = session_version + 1 \
             WHERE id = $1 RETURNING session_version",
        )
        .bind(id.as_uuid())
        .bind(password_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)
    }

    async fn create_first(
        &self,
        credentials: &Credentials,
        primary_member: &Member,
    ) -> RepoResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        // Insertion conditionnelle : atomique même en cas d'appels concurrents.
        let created = sqlx::query(
            "INSERT INTO users (id, email, password_hash, name) \
             SELECT $1, $2, $3, $4 WHERE NOT EXISTS (SELECT 1 FROM users)",
        )
        .bind(credentials.user.id.as_uuid())
        .bind(&credentials.user.email)
        .bind(&credentials.password_hash)
        .bind(&credentials.user.name)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        if created == 0 {
            return Ok(false);
        }
        sqlx::query(
            "INSERT INTO members (id, name, color, is_primary) \
             VALUES ($1, $2, $3, NOT EXISTS (SELECT 1 FROM members WHERE is_primary)) ON CONFLICT (name) DO NOTHING",
        )
        .bind(primary_member.id.as_uuid())
        .bind(&primary_member.name)
        .bind(&primary_member.color)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }
}

// ---------------------------------------------------------------- journal des connexions

#[derive(sqlx::FromRow)]
struct LoginEventRow {
    at: DateTime<Utc>,
    action: String,
    success: bool,
    ip: Option<String>,
    user_agent: Option<String>,
}

impl LoginEventRow {
    fn into_event(self) -> RepoResult<LoginEvent> {
        let action = LoginAction::parse(&self.action)
            .ok_or_else(|| RepoError::Backend(format!("action inconnue : {}", self.action)))?;
        Ok(LoginEvent {
            at: self.at,
            action,
            success: self.success,
            origin: LoginOrigin {
                ip: self.ip,
                user_agent: self.user_agent,
            },
        })
    }
}

#[async_trait]
impl LoginJournal for PgStore {
    async fn record(&self, event: &LoginEvent) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO login_events (at, action, success, ip, user_agent) VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(event.at)
        .bind(event.action.as_str())
        .bind(event.success)
        .bind(&event.origin.ip)
        .bind(&event.origin.user_agent)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn recent(&self, limit: i64) -> RepoResult<Vec<LoginEvent>> {
        let rows: Vec<LoginEventRow> = sqlx::query_as(
            "SELECT at, action, success, ip, user_agent FROM login_events \
             ORDER BY at DESC, id DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter().map(LoginEventRow::into_event).collect()
    }

    async fn prune_before(&self, before: DateTime<Utc>) -> RepoResult<u64> {
        Ok(sqlx::query("DELETE FROM login_events WHERE at < $1")
            .bind(before)
            .execute(&self.pool)
            .await
            .map_err(db)?
            .rows_affected())
    }
}

// ---------------------------------------------------------------- patrimoine

#[async_trait]
impl PortfolioRepository for PgStore {
    async fn load(&self) -> RepoResult<PortfolioData> {
        let pool = &self.pool;
        let account_rows = sqlx::query_as::<_, AccountRow>(
            "SELECT id, name, institution, kind, envelope, currency, cash_balance, annual_yield_pct, source, \
             external_id, connection_id, opened_at, updated_at FROM accounts ORDER BY created_at, name",
        )
        .fetch_all(pool)
        .await
        .map_err(db)?;
        let owner_rows = sqlx::query_as::<_, OwnerRow>(
            "SELECT o.account_id, o.member_id, o.share FROM account_owners o JOIN members m ON m.id = o.member_id \
             ORDER BY m.is_primary DESC, m.created_at, m.name",
        )
        .fetch_all(pool)
        .await
        .map_err(db)?;
        let holding_rows = sqlx::query_as::<_, HoldingRow>(
            "SELECT id, account_id, kind, name, isin, symbol, security_type, quantity, buy_price, current_price, \
             currency, expense_ratio, price_source, price_updated_at FROM holdings ORDER BY created_at, name",
        )
        .fetch_all(pool)
        .await
        .map_err(db)?;
        let property_rows = sqlx::query_as::<_, PropertyRow>(
            "SELECT id, account_id, address, category, building_type, surface_m2, purchase_price, purchase_fees, \
             purchased_at, current_value, monthly_rent, monthly_charges, yearly_property_tax FROM properties",
        )
        .fetch_all(pool)
        .await
        .map_err(db)?;
        let loan_rows = sqlx::query_as::<_, LoanRow>(
            "SELECT id, account_id, property_account_id, principal, interest_rate_pct, insurance_rate_pct, \
             duration_months, start_date FROM loans",
        )
        .fetch_all(pool)
        .await
        .map_err(db)?;

        let mut owners: HashMap<Uuid, Vec<(Uuid, f64)>> = HashMap::new();
        for row in owner_rows {
            owners
                .entry(row.account_id)
                .or_default()
                .push((row.member_id, row.share));
        }
        let mut properties = HashMap::new();
        for row in property_rows {
            properties.insert(row.account_id, Property::try_from(row)?);
        }
        let mut loans = HashMap::new();
        for row in loan_rows {
            loans.insert(row.account_id, Loan::try_from(row)?);
        }
        let mut accounts = Vec::with_capacity(account_rows.len());
        for row in account_rows {
            let id = row.id;
            accounts.push(row.into_account(
                owners.remove(&id).unwrap_or_default(),
                properties.remove(&id),
                loans.remove(&id),
            )?);
        }
        Ok(PortfolioData {
            members: self.members().await?,
            accounts,
            holdings: holding_rows
                .into_iter()
                .map(Holding::try_from)
                .collect::<RepoResult<_>>()?,
        })
    }

    async fn members(&self) -> RepoResult<Vec<Member>> {
        let rows = sqlx::query_as::<_, MemberRow>(
            "SELECT id, name, color, is_primary FROM members ORDER BY is_primary DESC, created_at, name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(Member::from).collect())
    }

    async fn insert_member(&self, member: &Member) -> RepoResult<()> {
        sqlx::query("INSERT INTO members (id, name, color, is_primary) VALUES ($1, $2, $3, $4)")
            .bind(member.id.as_uuid())
            .bind(&member.name)
            .bind(&member.color)
            .bind(member.is_primary)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn update_member(&self, member: &Member) -> RepoResult<()> {
        sqlx::query("UPDATE members SET name = $2, color = $3 WHERE id = $1")
            .bind(member.id.as_uuid())
            .bind(&member.name)
            .bind(&member.color)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn delete_member(&self, id: MemberId) -> RepoResult<()> {
        sqlx::query("DELETE FROM members WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(())
    }

    async fn save_accounts(&self, writes: &[AccountWrite]) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for write in writes {
            save_account(&mut tx, write).await?;
        }
        tx.commit().await.map_err(db)
    }

    async fn delete_account(&self, id: AccountId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM accounts WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }

    async fn save_holding(&self, holding: &Holding, at: DateTime<Utc>) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        upsert_holding(&mut tx, holding).await?;
        touch_account(&mut tx, holding.account_id.as_uuid(), at).await?;
        tx.commit().await.map_err(db)
    }

    async fn delete_holding(&self, id: HoldingId, at: DateTime<Utc>) -> RepoResult<bool> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let account_id: Option<Uuid> =
            sqlx::query_scalar("DELETE FROM holdings WHERE id = $1 RETURNING account_id")
                .bind(id.as_uuid())
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?;
        let Some(account_id) = account_id else {
            return Ok(false);
        };
        touch_account(&mut tx, account_id, at).await?;
        tx.commit().await.map_err(db)?;
        Ok(true)
    }

    async fn update_prices(
        &self,
        prices: &[(HoldingId, f64)],
        at: DateTime<Utc>,
    ) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for (id, price) in prices {
            sqlx::query(
                "UPDATE holdings SET current_price = $2, price_source = 'auto', price_updated_at = $3 WHERE id = $1",
            )
            .bind(id.as_uuid())
            .bind(price)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
            sqlx::query(
                "INSERT INTO price_history (holding_id, date, price, currency) \
                 SELECT id, $2, $3, currency FROM holdings WHERE id = $1 \
                 ON CONFLICT (holding_id, date) DO UPDATE SET price = EXCLUDED.price, currency = EXCLUDED.currency",
            )
            .bind(id.as_uuid())
            .bind(at.date_naive())
            .bind(price)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn price_history(
        &self,
        holding: HoldingId,
        from: NaiveDate,
    ) -> RepoResult<Vec<PricePoint>> {
        let rows: Vec<(NaiveDate, f64, String)> = sqlx::query_as(
            "SELECT date, price, currency FROM price_history WHERE holding_id = $1 AND date >= $2 ORDER BY date",
        )
        .bind(holding.as_uuid())
        .bind(from)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter()
            .map(|(date, price, currency)| {
                let currency = Currency::parse(&currency).map_err(|_| {
                    RepoError::Backend(format!("devise invalide en base : {currency}"))
                })?;
                Ok(PricePoint {
                    date,
                    price,
                    currency,
                })
            })
            .collect()
    }
}

// ---------------------------------------------------------------- taux, mouvements, instantanés

#[async_trait]
impl FxRateRepository for PgStore {
    async fn rates(&self) -> RepoResult<(FxRates, Option<DateTime<Utc>>)> {
        let rows: Vec<(String, f64, DateTime<Utc>)> =
            sqlx::query_as("SELECT currency, rate, updated_at FROM fx_rates")
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
        let updated_at = rows.iter().map(|(_, _, at)| *at).max();
        let rates = rows
            .into_iter()
            .filter_map(|(code, rate, _)| Currency::parse(&code).ok().map(|c| (c, rate)));
        Ok((FxRates::new(rates), updated_at))
    }

    async fn save_rates(&self, rates: &FxRates, at: DateTime<Utc>) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for (currency, rate) in rates.iter() {
            sqlx::query(
                "INSERT INTO fx_rates (currency, rate, updated_at) VALUES ($1, $2, $3) \
                 ON CONFLICT (currency) DO UPDATE SET rate = EXCLUDED.rate, updated_at = EXCLUDED.updated_at",
            )
            .bind(currency.as_str())
            .bind(rate)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }
}

#[async_trait]
impl TransactionRepository for PgStore {
    async fn page(
        &self,
        account: AccountId,
        limit: i64,
        offset: i64,
    ) -> RepoResult<(Vec<Transaction>, u64)> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT id, account_id, date, label, amount, category, manual_category, transfer_override \
             FROM transactions WHERE account_id = $1 \
             ORDER BY date DESC, created_at DESC, id LIMIT $2 OFFSET $3",
        )
        .bind(account.as_uuid())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let total: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM transactions WHERE account_id = $1")
                .bind(account.as_uuid())
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        Ok((
            rows.into_iter().map(Transaction::from).collect(),
            u64::try_from(total).unwrap_or(0),
        ))
    }

    async fn import(&self, account: &Account, transactions: &[Transaction]) -> RepoResult<u64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        save_account(&mut tx, &AccountWrite::account_only(account.clone())).await?;
        let mut inserted = 0;
        for t in transactions {
            inserted += sqlx::query(
                "INSERT INTO transactions (id, account_id, date, label, amount, category) \
                 VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (account_id, date, label, amount) DO NOTHING",
            )
            .bind(t.id.as_uuid())
            .bind(t.account_id.as_uuid())
            .bind(t.date)
            .bind(&t.label)
            .bind(t.amount)
            .bind(&t.category)
            .execute(&mut *tx)
            .await
            .map_err(db)?
            .rows_affected();
        }
        tx.commit().await.map_err(db)?;
        Ok(inserted)
    }

    async fn delete(&self, id: TransactionId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM transactions WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }

    async fn between(&self, from: NaiveDate, to: NaiveDate) -> RepoResult<Vec<Transaction>> {
        let rows = sqlx::query_as::<_, TransactionRow>(
            "SELECT id, account_id, date, label, amount, category, manual_category, transfer_override \
             FROM transactions WHERE date BETWEEN $1 AND $2 ORDER BY date, created_at, id",
        )
        .bind(from)
        .bind(to)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows.into_iter().map(Transaction::from).collect())
    }

    async fn by_id(&self, id: TransactionId) -> RepoResult<Option<Transaction>> {
        let row = sqlx::query_as::<_, TransactionRow>(
            "SELECT id, account_id, date, label, amount, category, manual_category, transfer_override \
             FROM transactions WHERE id = $1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        Ok(row.map(Transaction::from))
    }

    async fn annotate(
        &self,
        transaction: &Transaction,
        rule: Option<&CategoryRule>,
    ) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "UPDATE transactions SET manual_category = $2, transfer_override = $3 WHERE id = $1",
        )
        .bind(transaction.id.as_uuid())
        .bind(&transaction.manual_category)
        .bind(transaction.transfer_override)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if let Some(rule) = rule {
            sqlx::query(
                "INSERT INTO budget_rules (id, pattern, category) VALUES ($1, $2, $3) \
                 ON CONFLICT (pattern) DO UPDATE SET category = EXCLUDED.category, updated_at = now()",
            )
            .bind(rule.id.as_uuid())
            .bind(rule.pattern())
            .bind(rule.category())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn categories(&self) -> RepoResult<Vec<String>> {
        sqlx::query_scalar(
            "SELECT category FROM transactions WHERE category IS NOT NULL \
             UNION SELECT manual_category FROM transactions WHERE manual_category IS NOT NULL \
             ORDER BY 1",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)
    }
}

#[async_trait]
impl BudgetRepository for PgStore {
    async fn rules(&self) -> RepoResult<Vec<CategoryRule>> {
        sqlx::query_as::<_, BudgetRuleRow>(
            "SELECT id, pattern, category FROM budget_rules ORDER BY pattern",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(CategoryRule::try_from)
        .collect()
    }

    async fn delete_rule(&self, id: BudgetRuleId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM budget_rules WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }

    async fn targets(&self) -> RepoResult<Vec<BudgetTarget>> {
        sqlx::query_as::<_, BudgetTargetRow>(
            "SELECT category, monthly_amount FROM budget_targets ORDER BY category",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(BudgetTarget::try_from)
        .collect()
    }

    async fn save_target(&self, target: &BudgetTarget) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO budget_targets (category, monthly_amount) VALUES ($1, $2) \
             ON CONFLICT (category) DO UPDATE SET monthly_amount = EXCLUDED.monthly_amount",
        )
        .bind(target.category())
        .bind(target.monthly_amount())
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete_target(&self, category: &str) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM budget_targets WHERE category = $1")
            .bind(category)
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }
}

/// Colonnes lues d'une opération (macro : sqlx n'accepte que des requêtes littérales).
macro_rules! select_operations {
    ($condition:literal) => {
        concat!(
            "SELECT id, account_id, holding_id, kind, date, quantity, unit_price, amount, fees, \
             currency, note, recorded_at FROM investment_operations ",
            $condition
        )
    };
}

async fn upsert_operation(conn: &mut PgConnection, op: &Operation) -> RepoResult<()> {
    let movement = &op.movement;
    let trade = movement.trade();
    sqlx::query(
        "INSERT INTO investment_operations (id, account_id, holding_id, kind, date, quantity, unit_price, \
         amount, fees, currency, note, recorded_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
         ON CONFLICT (id) DO UPDATE SET holding_id = EXCLUDED.holding_id, kind = EXCLUDED.kind, \
         date = EXCLUDED.date, quantity = EXCLUDED.quantity, unit_price = EXCLUDED.unit_price, \
         amount = EXCLUDED.amount, fees = EXCLUDED.fees, currency = EXCLUDED.currency, note = EXCLUDED.note",
    )
    .bind(op.id.as_uuid())
    .bind(op.account_id.as_uuid())
    .bind(movement.holding().map(|id| id.as_uuid()))
    .bind(movement.kind().as_str())
    .bind(op.date)
    .bind(trade.map(|t| t.quantity()))
    .bind(trade.map(|t| t.unit_price()))
    .bind(trade.is_none().then(|| movement.amount()))
    .bind(trade.map_or(0.0, |t| t.fees()))
    .bind(op.currency.as_str())
    .bind(&op.note)
    .bind(op.recorded_at)
    .execute(conn)
    .await
    .map_err(db)?;
    Ok(())
}

#[async_trait]
impl OperationRepository for PgStore {
    async fn operations(&self, account: AccountId) -> RepoResult<Vec<Operation>> {
        sqlx::query_as::<_, OperationRow>(select_operations!(
            "WHERE account_id = $1 ORDER BY date, recorded_at, id"
        ))
        .bind(account.as_uuid())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?
        .into_iter()
        .map(Operation::try_from)
        .collect()
    }

    async fn find_operation(&self, id: OperationId) -> RepoResult<Option<Operation>> {
        sqlx::query_as::<_, OperationRow>(select_operations!("WHERE id = $1"))
            .bind(id.as_uuid())
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .map(Operation::try_from)
            .transpose()
    }

    async fn apply_operation(
        &self,
        change: &OperationChange,
        positions: &[PositionUpdate],
        at: DateTime<Utc>,
    ) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let account = match change {
            OperationChange::Save(operation) => {
                upsert_operation(&mut tx, operation).await?;
                operation.account_id
            }
            OperationChange::Delete { id, account } => {
                sqlx::query("DELETE FROM investment_operations WHERE id = $1")
                    .bind(id.as_uuid())
                    .execute(&mut *tx)
                    .await
                    .map_err(db)?;
                *account
            }
        };
        for position in positions {
            sqlx::query("UPDATE holdings SET quantity = $2, buy_price = $3 WHERE id = $1")
                .bind(position.holding.as_uuid())
                .bind(position.quantity)
                .bind(position.buy_price)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        touch_account(&mut tx, account.as_uuid(), at).await?;
        tx.commit().await.map_err(db)
    }
}

#[async_trait]
impl SnapshotRepository for PgStore {
    async fn upsert(
        &self,
        date: NaiveDate,
        member: Option<MemberId>,
        snapshot: &Snapshot,
    ) -> RepoResult<()> {
        let member = member.map(|m| m.as_uuid());
        let totals = snapshot.totals;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query(
            "INSERT INTO snapshots (date, member_id, gross_assets, liabilities, net_worth, detailed) \
             VALUES ($1, $2, $3, $4, $5, true) \
             ON CONFLICT (date, member_id) DO UPDATE SET gross_assets = EXCLUDED.gross_assets, \
             liabilities = EXCLUDED.liabilities, net_worth = EXCLUDED.net_worth, detailed = true, \
             corrected = false",
        )
        .bind(date)
        .bind(member)
        .bind(totals.gross_assets)
        .bind(totals.liabilities)
        .bind(totals.net_worth)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        // Le détail du jour est remplacé en entier : un compte supprimé dans la journée disparaît.
        sqlx::query(
            "DELETE FROM snapshot_accounts WHERE date = $1 AND member_id IS NOT DISTINCT FROM $2",
        )
        .bind(date)
        .bind(member)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        for line in &snapshot.accounts {
            sqlx::query(
                "INSERT INTO snapshot_accounts (date, member_id, account_id, name, kind, value, gain) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(date)
            .bind(member)
            .bind(line.account_id.as_uuid())
            .bind(&line.name)
            .bind(line.kind.as_str())
            .bind(line.value)
            .bind(line.gain)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn since(
        &self,
        member: Option<MemberId>,
        from: NaiveDate,
    ) -> RepoResult<Vec<SnapshotPoint>> {
        let member = member.map(|m| m.as_uuid());
        let rows: Vec<(NaiveDate, f64, f64, f64, bool, bool)> = sqlx::query_as(
            "SELECT date, gross_assets, liabilities, net_worth, detailed, corrected FROM snapshots \
             WHERE member_id IS NOT DISTINCT FROM $1 AND date >= $2 ORDER BY date",
        )
        .bind(member)
        .bind(from)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let lines: Vec<SnapshotAccountRow> = sqlx::query_as(
            "SELECT date, account_id, name, kind, value, gain FROM snapshot_accounts \
             WHERE member_id IS NOT DISTINCT FROM $1 AND date >= $2 ORDER BY date, name, account_id",
        )
        .bind(member)
        .bind(from)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let mut by_date: HashMap<NaiveDate, Vec<AccountSnapshot>> = HashMap::new();
        for line in lines {
            let date = line.date;
            by_date.entry(date).or_default().push(line.try_into()?);
        }
        Ok(rows
            .into_iter()
            .map(
                |(date, gross_assets, liabilities, net_worth, detailed, corrected)| SnapshotPoint {
                    date,
                    totals: Totals {
                        gross_assets,
                        liabilities,
                        net_worth,
                    },
                    accounts: detailed.then(|| by_date.remove(&date).unwrap_or_default()),
                    corrected,
                },
            )
            .collect())
    }

    async fn correct(
        &self,
        date: NaiveDate,
        member: Option<MemberId>,
        totals: Totals,
    ) -> RepoResult<bool> {
        let member = member.map(|m| m.as_uuid());
        let mut tx = self.pool.begin().await.map_err(db)?;
        let updated = sqlx::query(
            "UPDATE snapshots SET gross_assets = $3, liabilities = $4, net_worth = $5, \
             detailed = false, corrected = true \
             WHERE date = $1 AND member_id IS NOT DISTINCT FROM $2",
        )
        .bind(date)
        .bind(member)
        .bind(totals.gross_assets)
        .bind(totals.liabilities)
        .bind(totals.net_worth)
        .execute(&mut *tx)
        .await
        .map_err(db)?
        .rows_affected();
        // Le détail ne correspondrait plus aux totaux saisis.
        sqlx::query(
            "DELETE FROM snapshot_accounts WHERE date = $1 AND member_id IS NOT DISTINCT FROM $2",
        )
        .bind(date)
        .bind(member)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(updated > 0)
    }

    async fn delete_range(&self, range: DateRange) -> RepoResult<u64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM snapshot_accounts WHERE date BETWEEN $1 AND $2")
            .bind(range.from())
            .bind(range.to())
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let (days,): (i64,) = sqlx::query_as(
            "WITH gone AS (DELETE FROM snapshots WHERE date BETWEEN $1 AND $2 RETURNING date) \
             SELECT COUNT(DISTINCT date) FROM gone",
        )
        .bind(range.from())
        .bind(range.to())
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(days.unsigned_abs())
    }

    async fn delete_all(&self) -> RepoResult<u64> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("DELETE FROM snapshot_accounts")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let (days,): (i64,) = sqlx::query_as(
            "WITH gone AS (DELETE FROM snapshots RETURNING date) SELECT COUNT(DISTINCT date) FROM gone",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(days.unsigned_abs())
    }
}

// ---------------------------------------------------------------- objectifs, connexions, import

#[async_trait]
impl GoalRepository for PgStore {
    async fn list(&self) -> RepoResult<Vec<Goal>> {
        let rows = sqlx::query_as::<_, GoalRow>(
            "SELECT id, name, icon, kind, target_amount, target_date, monthly_contribution, expected_return_pct, \
             monthly_expenses, coverage_months FROM goals ORDER BY created_at, name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let account_links: Vec<(Uuid, Uuid)> =
            sqlx::query_as("SELECT goal_id, account_id FROM goal_accounts ORDER BY account_id")
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
        let member_links: Vec<(Uuid, Uuid)> =
            sqlx::query_as("SELECT goal_id, member_id FROM goal_members ORDER BY member_id")
                .fetch_all(&self.pool)
                .await
                .map_err(db)?;
        rows.into_iter()
            .map(|row| {
                let linked = |links: &[(Uuid, Uuid)]| -> Vec<Uuid> {
                    links
                        .iter()
                        .filter(|(goal, _)| *goal == row.id)
                        .map(|(_, id)| *id)
                        .collect()
                };
                let accounts = linked(&account_links)
                    .into_iter()
                    .map(AccountId::from_uuid)
                    .collect();
                let members = linked(&member_links)
                    .into_iter()
                    .map(MemberId::from_uuid)
                    .collect();
                row.into_goal(accounts, members)
            })
            .collect()
    }

    async fn save(&self, goal: &Goal) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        save_goal(&mut tx, goal).await?;
        tx.commit().await.map_err(db)
    }

    async fn delete(&self, id: GoalId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM goals WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }
}

const CONNECTION_COLUMNS: &str =
    "SELECT id, provider, institution, status, last_sync_at, error, external_id FROM connections";

#[async_trait]
impl ConnectionRepository for PgStore {
    async fn list(&self) -> RepoResult<Vec<Connection>> {
        let rows = sqlx::query_as::<_, ConnectionRow>(sqlx::AssertSqlSafe(format!(
            "{CONNECTION_COLUMNS} ORDER BY created_at"
        )))
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.into_iter().map(Connection::try_from).collect()
    }

    async fn find(&self, id: ConnectionId) -> RepoResult<Option<Connection>> {
        let row = sqlx::query_as::<_, ConnectionRow>(sqlx::AssertSqlSafe(format!(
            "{CONNECTION_COLUMNS} WHERE id = $1"
        )))
        .bind(id.as_uuid())
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;
        row.map(Connection::try_from).transpose()
    }

    async fn save(&self, c: &Connection) -> RepoResult<()> {
        sqlx::query(
            "INSERT INTO connections (id, provider, institution, status, last_sync_at, error, external_id) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (id) DO UPDATE SET provider = EXCLUDED.provider, \
             institution = EXCLUDED.institution, status = EXCLUDED.status, last_sync_at = EXCLUDED.last_sync_at, \
             error = EXCLUDED.error, external_id = EXCLUDED.external_id",
        )
        .bind(c.id.as_uuid())
        .bind(&c.provider)
        .bind(&c.institution)
        .bind(c.status.as_str())
        .bind(c.last_sync_at)
        .bind(&c.error)
        .bind(&c.external_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        Ok(())
    }

    async fn delete(&self, id: ConnectionId) -> RepoResult<bool> {
        let result = sqlx::query("DELETE FROM connections WHERE id = $1")
            .bind(id.as_uuid())
            .execute(&self.pool)
            .await
            .map_err(db)?;
        Ok(result.rows_affected() > 0)
    }
}

#[async_trait]
impl ImportRepository for PgStore {
    async fn apply(&self, batch: &ImportBatch) -> RepoResult<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        for member in &batch.members {
            upsert_member(&mut tx, member).await?;
        }
        for write in &batch.accounts {
            save_account(&mut tx, write).await?;
        }
        for goal in &batch.goals {
            save_goal(&mut tx, goal).await?;
        }
        tx.commit().await.map_err(db)
    }
}
