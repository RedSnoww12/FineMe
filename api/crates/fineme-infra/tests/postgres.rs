//! Tests d'intégration de l'adaptateur Postgres contre un vrai Postgres (testcontainers).
//! Chaque test démarre son propre conteneur jetable ; toutes les données sont fictives.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use fineme_application::ports::{
    AccountWrite, BudgetRepository, ConnectionRepository, Credentials, FxRateRepository,
    GoalRepository, ImportBatch, ImportProfileRepository, ImportRepository, LoginJournal,
    OperationChange, OperationRepository, PortfolioRepository, PositionUpdate, PricePoint,
    RepoError, SnapshotRepository, TransactionRepository, UserRepository,
};
use fineme_domain::account::{Account, AccountKind, AccountSource};
use fineme_domain::budget::{BudgetTarget, CategoryRule};
use fineme_domain::connection::{Connection, ConnectionStatus};
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::goal::{Goal, GoalKind};
use fineme_domain::history::DateRange;
use fineme_domain::holding::{Holding, HoldingKind, PriceSource};
use fineme_domain::ids::*;
use fineme_domain::import_profile::{ImportKind, ImportProfile};
use fineme_domain::loan::{Loan, LoanTerms};
use fineme_domain::login::{LoginAction, LoginEvent, LoginOrigin};
use fineme_domain::member::{Member, User};
use fineme_domain::operation::{Amount, Movement, MovementFields, Operation, OperationKind, Trade};
use fineme_domain::ownership::Ownership;
use fineme_domain::portfolio::{AccountSnapshot, Snapshot, Totals};
use fineme_domain::property::{BuildingType, Property, PropertyCategory};
use fineme_domain::transaction::Transaction;
use fineme_infra::postgres::{self, PgStore};
use sqlx::PgPool;
use testcontainers_modules::postgres::Postgres;
use testcontainers_modules::testcontainers::runners::AsyncRunner;
use testcontainers_modules::testcontainers::{ContainerAsync, ImageExt};

struct TestDb {
    store: PgStore,
    pool: PgPool,
    _container: ContainerAsync<Postgres>,
}

async fn test_db() -> TestDb {
    let container = Postgres::default()
        .with_tag("17")
        .start()
        .await
        .expect("démarrage du conteneur Postgres");
    let port = container
        .get_host_port_ipv4(5432)
        .await
        .expect("port du conteneur");
    let host = container.get_host().await.expect("hôte du conteneur");
    let pool = postgres::connect(&format!(
        "postgres://postgres:postgres@{host}:{port}/postgres"
    ))
    .await
    .unwrap();
    postgres::migrate(&pool).await.unwrap();
    TestDb {
        store: PgStore::new(pool.clone()),
        pool,
        _container: container,
    }
}

fn at(day: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, day, 9, 30, 0).unwrap()
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn member(name: &str, is_primary: bool) -> Member {
    Member {
        id: MemberId::generate(),
        name: name.into(),
        color: "#6366f1".into(),
        is_primary,
    }
}

async fn household(db: &TestDb) -> (Member, Member) {
    let (alice, bob) = (member("Alice", true), member("Bob", false));
    db.store.insert_member(&alice).await.unwrap();
    db.store.insert_member(&bob).await.unwrap();
    (alice, bob)
}

fn account(kind: AccountKind, name: &str, owners: &[(MemberId, f64)]) -> Account {
    Account {
        id: AccountId::generate(),
        name: name.into(),
        institution: Some("Banque Fictive".into()),
        kind,
        envelope: None,
        currency: Currency::eur(),
        cash_balance: 1500.5,
        annual_yield_pct: Some(1.7),
        source: AccountSource::Manual,
        external_id: None,
        connection_id: None,
        opened_at: Some(date(2020, 1, 15)),
        updated_at: at(1),
        owners: Ownership::from_fractions(owners.iter().copied()).unwrap(),
        property: None,
        loan: None,
    }
}

fn property() -> Property {
    Property {
        id: PropertyId::generate(),
        address: Some("1 rue des Exemples".into()),
        category: PropertyCategory::Rental,
        building_type: BuildingType::House,
        surface_m2: Some(90.0),
        purchase_price: 250_000.0,
        purchase_fees: 20_000.0,
        purchased_at: Some(date(2022, 3, 1)),
        current_value: 300_000.0,
        monthly_rent: 1200.0,
        monthly_charges: 100.0,
        yearly_property_tax: 1200.0,
    }
}

fn holding(account_id: AccountId, name: &str) -> Holding {
    Holding {
        id: HoldingId::generate(),
        account_id,
        kind: HoldingKind::Security,
        name: name.into(),
        isin: Some("FR0000000001".into()),
        symbol: Some("DEMO".into()),
        security_type: Some("etf".into()),
        quantity: 10.0,
        buy_price: Some(20.0),
        current_price: 25.0,
        currency: Currency::parse("USD").unwrap(),
        expense_ratio: Some(0.002),
        price_source: PriceSource::Manual,
        price_updated_at: Some(at(1)),
    }
}

async fn save(db: &TestDb, account: &Account) {
    db.store
        .save_accounts(&[AccountWrite::account_only(account.clone())])
        .await
        .unwrap();
}

async fn count(db: &TestDb, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(&db.pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn migrations_are_idempotent_and_start_empty() {
    let db = test_db().await;
    postgres::migrate(&db.pool).await.unwrap();
    let data = db.store.load().await.unwrap();
    assert!(data.members.is_empty() && data.accounts.is_empty() && data.holdings.is_empty());
    assert!(!db.store.exists_any().await.unwrap());
    let (rates, updated_at) = db.store.rates().await.unwrap();
    assert!(rates.is_empty() && updated_at.is_none());
}

#[tokio::test]
async fn first_user_is_created_once_with_its_primary_member() {
    let db = test_db().await;
    let credentials = Credentials {
        user: User {
            id: UserId::generate(),
            email: "camille@exemple.test".into(),
            name: "Camille".into(),
        },
        password_hash: "$argon2id$fictif".into(),
        session_version: 0,
    };
    assert!(
        db.store
            .create_first(&credentials, &member("Camille", true))
            .await
            .unwrap()
    );
    assert!(db.store.exists_any().await.unwrap());

    let other = Credentials {
        user: User {
            id: UserId::generate(),
            email: "autre@exemple.test".into(),
            name: "Autre".into(),
        },
        password_hash: "x".into(),
        session_version: 0,
    };
    assert!(
        !db.store
            .create_first(&other, &member("Autre", true))
            .await
            .unwrap()
    );
    assert_eq!(count(&db, "users").await, 1);

    assert_eq!(
        db.store
            .find_by_email("camille@exemple.test")
            .await
            .unwrap(),
        Some(credentials.clone())
    );
    assert_eq!(
        db.store.find_by_email("autre@exemple.test").await.unwrap(),
        None
    );
    assert_eq!(
        db.store.find_by_id(credentials.user.id).await.unwrap(),
        Some(credentials.clone())
    );
    assert_eq!(db.store.find_by_id(UserId::generate()).await.unwrap(), None);
    let members = db.store.members().await.unwrap();
    assert_eq!(members.len(), 1);
    assert!(members[0].is_primary && members[0].name == "Camille");
}

#[tokio::test]
async fn login_journal_lists_newest_first_and_prunes_old_entries() {
    let db = test_db().await;
    let event = |day: u32, action: LoginAction, success: bool| LoginEvent {
        at: at(day),
        action,
        success,
        origin: LoginOrigin::new(Some("192.0.2.10"), Some("Navigateur fictif")),
    };
    for e in [
        event(1, LoginAction::Login, false),
        event(3, LoginAction::Reset, true),
        event(2, LoginAction::Login, true),
    ] {
        db.store.record(&e).await.unwrap();
    }
    db.store
        .record(&LoginEvent {
            origin: LoginOrigin::default(),
            ..event(4, LoginAction::Login, false)
        })
        .await
        .unwrap();

    let recent = db.store.recent(3).await.unwrap();
    let days: Vec<_> = recent.iter().map(|e| e.at).collect();
    assert_eq!(days, vec![at(4), at(3), at(2)]);
    assert_eq!(recent[0].origin, LoginOrigin::default());
    assert_eq!(recent[1], event(3, LoginAction::Reset, true));

    assert_eq!(db.store.prune_before(at(3)).await.unwrap(), 2);
    assert_eq!(db.store.recent(10).await.unwrap().len(), 2);
}

#[tokio::test]
async fn password_update_replaces_hash_and_bumps_session_version() {
    let db = test_db().await;
    let credentials = Credentials {
        user: User {
            id: UserId::generate(),
            email: "camille@exemple.test".into(),
            name: "Camille".into(),
        },
        password_hash: "$argon2id$ancien".into(),
        session_version: 0,
    };
    db.store
        .create_first(&credentials, &member("Camille", true))
        .await
        .unwrap();

    for (expected_version, hash) in [(1, "$argon2id$nouveau"), (2, "$argon2id$encore")] {
        assert_eq!(
            db.store
                .update_password(credentials.user.id, hash)
                .await
                .unwrap(),
            Some(expected_version)
        );
        let stored = db
            .store
            .find_by_email("camille@exemple.test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.password_hash, hash);
        assert_eq!(stored.session_version, expected_version);
    }
    assert_eq!(
        db.store
            .update_password(UserId::generate(), "$argon2id$x")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn members_are_unique_by_name_and_protected_while_they_own_accounts() {
    let db = test_db().await;
    let (alice, bob) = household(&db).await;
    assert_eq!(
        db.store.insert_member(&member("Bob", false)).await,
        Err(RepoError::Conflict)
    );
    let renamed = Member {
        name: "Alice".into(),
        ..bob.clone()
    };
    assert_eq!(
        db.store.update_member(&renamed).await,
        Err(RepoError::Conflict)
    );

    let updated = Member {
        name: "Robert".into(),
        color: "#00ff00".into(),
        ..bob.clone()
    };
    db.store.update_member(&updated).await.unwrap();
    // Le membre principal vient toujours en premier.
    assert_eq!(
        db.store.members().await.unwrap(),
        vec![alice.clone(), updated]
    );

    save(
        &db,
        &account(AccountKind::Checking, "Courant", &[(bob.id, 1.0)]),
    )
    .await;
    assert_eq!(
        db.store.delete_member(bob.id).await,
        Err(RepoError::Reference)
    );
    assert_eq!(db.store.members().await.unwrap().len(), 2);
}

#[tokio::test]
async fn account_aggregate_round_trips_with_property_loan_and_partial_shares() {
    let db = test_db().await;
    let (alice, bob) = household(&db).await;
    let mut house = account(
        AccountKind::RealEstate,
        "Maison",
        &[(alice.id, 0.5), (bob.id, 0.3)],
    );
    house.property = Some(property());
    house.envelope = Some("sci".into());
    house.external_id = Some("maison-1".into());
    house.source = AccountSource::Import;
    let mut loan = account(
        AccountKind::Loan,
        "Prêt maison",
        &[(alice.id, 0.5), (bob.id, 0.5)],
    );
    loan.loan = Some(Loan {
        id: LoanId::generate(),
        property_account_id: Some(house.id),
        terms: LoanTerms::new(200_000.0, 4.0, 0.3, 240, date(2026, 6, 1)).unwrap(),
    });
    db.store
        .save_accounts(&[
            AccountWrite::account_only(house.clone()),
            AccountWrite::account_only(loan.clone()),
        ])
        .await
        .unwrap();

    let data = db.store.load().await.unwrap();
    let stored = |id: AccountId| data.accounts.iter().find(|a| a.id == id).cloned().unwrap();
    assert_eq!(stored(house.id), house);
    assert_eq!(stored(loan.id), loan);

    // Mise à jour : les détenteurs sont remplacés, le bien modifié, la date de mise à jour suivie.
    let mut changed = house.clone();
    changed.owners = Ownership::sole(bob.id);
    changed.updated_at = at(2);
    changed.institution = None;
    if let Some(p) = &mut changed.property {
        p.current_value = 310_000.0;
        p.address = None;
    }
    save(&db, &changed).await;
    let reloaded = db
        .store
        .load()
        .await
        .unwrap()
        .accounts
        .into_iter()
        .find(|a| a.id == house.id)
        .unwrap();
    assert_eq!(reloaded, changed);
    assert_eq!(count(&db, "account_owners").await, 3);

    // Supprimer le bien délie le prêt (clé étrangère) sans le supprimer.
    assert!(db.store.delete_account(house.id).await.unwrap());
    assert!(!db.store.delete_account(house.id).await.unwrap());
    let remaining = db.store.load().await.unwrap().accounts;
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0].loan.as_ref().unwrap().property_account_id,
        None
    );
    assert_eq!(count(&db, "properties").await, 0);
}

#[tokio::test]
async fn changing_kind_removes_property_and_loan_rows() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let mut house = account(AccountKind::RealEstate, "Maison", &[(alice.id, 1.0)]);
    house.property = Some(property());
    save(&db, &house).await;
    let plain = Account {
        kind: AccountKind::Other,
        property: None,
        ..house.clone()
    };
    save(&db, &plain).await;
    assert_eq!(count(&db, "properties").await, 0);
    assert_eq!(db.store.load().await.unwrap().accounts, vec![plain]);
}

#[tokio::test]
async fn save_accounts_is_all_or_nothing() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let valid = account(AccountKind::Savings, "Livret", &[(alice.id, 1.0)]);
    let orphan = account(
        AccountKind::Savings,
        "Orphelin",
        &[(MemberId::generate(), 1.0)],
    );
    let result = db
        .store
        .save_accounts(&[
            AccountWrite::account_only(valid),
            AccountWrite::account_only(orphan),
        ])
        .await;
    assert_eq!(result, Err(RepoError::Reference));
    assert_eq!(count(&db, "accounts").await, 0);
}

#[tokio::test]
async fn holdings_are_saved_replaced_and_repriced() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let pea = account(AccountKind::Investment, "PEA", &[(alice.id, 1.0)]);
    save(&db, &pea).await;
    let (first, second) = (holding(pea.id, "ETF A"), holding(pea.id, "ETF B"));
    db.store.save_holding(&first, at(2)).await.unwrap();
    db.store.save_holding(&second, at(3)).await.unwrap();
    let data = db.store.load().await.unwrap();
    assert_eq!(data.holdings, vec![first.clone(), second.clone()]);
    assert_eq!(data.accounts[0].updated_at, at(3));

    // Mise à jour d'une position existante.
    let edited = Holding {
        quantity: 12.0,
        buy_price: None,
        isin: None,
        ..first.clone()
    };
    db.store.save_holding(&edited, at(4)).await.unwrap();

    // Remplacement de l'ensemble : « ETF A » est conservée, « ETF B » disparaît, « ETF C » arrive.
    let third = holding(pea.id, "ETF C");
    let write = AccountWrite {
        account: Account {
            source: AccountSource::Csv,
            ..pea.clone()
        },
        holdings: Some(vec![edited.clone(), third.clone()]),
    };
    db.store.save_accounts(&[write]).await.unwrap();
    let data = db.store.load().await.unwrap();
    assert_eq!(data.holdings, vec![edited.clone(), third.clone()]);
    assert_eq!(data.accounts[0].source, AccountSource::Csv);

    db.store
        .update_prices(&[(third.id, 31.5), (HoldingId::generate(), 1.0)], at(5))
        .await
        .unwrap();
    let repriced = db
        .store
        .load()
        .await
        .unwrap()
        .holdings
        .into_iter()
        .find(|h| h.id == third.id)
        .unwrap();
    assert_eq!(
        (
            repriced.current_price,
            repriced.price_source,
            repriced.price_updated_at
        ),
        (31.5, PriceSource::Auto, Some(at(5)))
    );

    assert!(db.store.delete_holding(edited.id, at(6)).await.unwrap());
    assert!(!db.store.delete_holding(edited.id, at(7)).await.unwrap());
    let data = db.store.load().await.unwrap();
    assert_eq!(data.holdings.len(), 1);
    assert_eq!(data.accounts[0].updated_at, at(6));
}

#[tokio::test]
async fn repricing_keeps_one_history_point_per_day_until_the_holding_is_deleted() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let pea = account(AccountKind::Investment, "PEA", &[(alice.id, 1.0)]);
    save(&db, &pea).await;
    let (etf, other) = (holding(pea.id, "ETF A"), holding(pea.id, "ETF B"));
    db.store.save_holding(&etf, at(2)).await.unwrap();
    db.store.save_holding(&other, at(2)).await.unwrap();

    // Deux cours le 5 (le second l'emporte), un le 6 ; une position inconnue est ignorée.
    for (price, day) in [(30.0, 5), (31.5, 5), (32.0, 6)] {
        db.store
            .update_prices(&[(etf.id, price), (HoldingId::generate(), 1.0)], at(day))
            .await
            .unwrap();
    }
    db.store
        .update_prices(&[(other.id, 7.0)], at(6))
        .await
        .unwrap();
    let usd = Currency::parse("USD").unwrap();
    assert_eq!(
        db.store
            .price_history(etf.id, date(2026, 1, 1))
            .await
            .unwrap(),
        vec![
            PricePoint {
                date: date(2026, 1, 5),
                price: 31.5,
                currency: usd.clone()
            },
            PricePoint {
                date: date(2026, 1, 6),
                price: 32.0,
                currency: usd.clone()
            },
        ]
    );
    assert_eq!(
        db.store
            .price_history(etf.id, date(2026, 1, 6))
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(count(&db, "price_history").await, 3);

    // L'historique disparaît avec la position, pas celui des autres.
    assert!(db.store.delete_holding(etf.id, at(7)).await.unwrap());
    assert!(
        db.store
            .price_history(etf.id, date(2026, 1, 1))
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(count(&db, "price_history").await, 1);
}

#[tokio::test]
async fn transactions_are_deduplicated_paged_and_deleted_with_their_account() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let checking = account(AccountKind::Checking, "Courant", &[(alice.id, 1.0)]);
    let other = account(AccountKind::Checking, "Autre", &[(alice.id, 1.0)]);
    save(&db, &checking).await;
    save(&db, &other).await;
    let tx = |account: AccountId, day: u32, label: &str, amount: f64| Transaction {
        id: TransactionId::generate(),
        account_id: account,
        date: date(2026, 1, day),
        label: label.into(),
        amount,
        category: Some("Divers".into()),
        manual_category: None,
        transfer_override: None,
    };
    let batch = vec![
        tx(checking.id, 1, "Courses", -54.2),
        tx(checking.id, 2, "Salaire", 2500.0),
        tx(checking.id, 1, "Courses", -54.2),
        tx(checking.id, 1, "Courses", -10.0),
    ];
    assert_eq!(db.store.import(&checking, &batch).await.unwrap(), 3);
    assert_eq!(db.store.import(&checking, &batch).await.unwrap(), 0);
    // Le même mouvement sur un autre compte n'est pas un doublon.
    assert_eq!(
        db.store
            .import(&other, &[tx(other.id, 1, "Courses", -54.2)])
            .await
            .unwrap(),
        1
    );

    let (page, total) = db.store.page(checking.id, 2, 0).await.unwrap();
    assert_eq!(total, 3);
    assert_eq!(page.len(), 2);
    assert_eq!(page[0].label, "Salaire");
    assert_eq!(page[0].category.as_deref(), Some("Divers"));
    let (rest, _) = db.store.page(checking.id, 2, 2).await.unwrap();
    assert_eq!(rest.len(), 1);

    assert!(
        TransactionRepository::delete(&db.store, page[0].id)
            .await
            .unwrap()
    );
    assert!(
        !TransactionRepository::delete(&db.store, page[0].id)
            .await
            .unwrap()
    );
    db.store.delete_account(checking.id).await.unwrap();
    assert_eq!(count(&db, "transactions").await, 1);
}

#[tokio::test]
async fn budget_annotations_rules_and_targets() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let checking = account(AccountKind::Checking, "Courant", &[(alice.id, 1.0)]);
    save(&db, &checking).await;
    let tx = |day: u32, label: &str, category: Option<&str>| Transaction {
        id: TransactionId::generate(),
        account_id: checking.id,
        date: date(2026, 1, day),
        label: label.into(),
        amount: -12.0,
        category: category.map(String::from),
        manual_category: None,
        transfer_override: None,
    };
    let batch = [
        tx(2, "CB BOUTIQUE", Some("Achats")),
        tx(20, "CB AUTRE", None),
    ];
    db.store.import(&checking, &batch).await.unwrap();

    let between = db
        .store
        .between(date(2026, 1, 1), date(2026, 1, 10))
        .await
        .unwrap();
    assert_eq!(between, vec![batch[0].clone()]);

    let annotated = Transaction {
        manual_category: Some("Loisirs".into()),
        transfer_override: Some(false),
        ..batch[0].clone()
    };
    let rule = CategoryRule::new(BudgetRuleId::generate(), "CB BOUTIQUE", "Loisirs").unwrap();
    db.store.annotate(&annotated, Some(&rule)).await.unwrap();
    assert_eq!(
        db.store.by_id(batch[0].id).await.unwrap(),
        Some(annotated.clone())
    );
    assert_eq!(
        db.store.by_id(TransactionId::generate()).await.unwrap(),
        None
    );
    // Même motif : la règle existante est mise à jour et garde son identifiant.
    let again = CategoryRule::new(BudgetRuleId::generate(), "cb boutique", "Cadeaux").unwrap();
    db.store.annotate(&annotated, Some(&again)).await.unwrap();
    let rules = db.store.rules().await.unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!((rules[0].id, rules[0].category()), (rule.id, "Cadeaux"));
    assert_eq!(
        db.store.categories().await.unwrap(),
        vec!["Achats".to_string(), "Loisirs".to_string()]
    );
    assert!(db.store.delete_rule(rule.id).await.unwrap());
    assert!(!db.store.delete_rule(rule.id).await.unwrap());

    db.store
        .save_target(&BudgetTarget::new("Loisirs", 100.0).unwrap())
        .await
        .unwrap();
    db.store
        .save_target(&BudgetTarget::new("Loisirs", 150.0).unwrap())
        .await
        .unwrap();
    let targets = db.store.targets().await.unwrap();
    assert_eq!(targets, vec![BudgetTarget::new("Loisirs", 150.0).unwrap()]);
    assert!(db.store.delete_target("Loisirs").await.unwrap());
    assert!(db.store.targets().await.unwrap().is_empty());
}

#[tokio::test]
async fn import_is_all_or_nothing() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let checking = account(AccountKind::Checking, "Courant", &[(alice.id, 1.0)]);
    save(&db, &checking).await;
    let tx = |account: AccountId, label: &str| Transaction {
        id: TransactionId::generate(),
        account_id: account,
        date: date(2026, 1, 1),
        label: label.into(),
        amount: -5.0,
        category: None,
        manual_category: None,
        transfer_override: None,
    };
    let updated = Account {
        cash_balance: 3500.0,
        source: AccountSource::Csv,
        ..checking.clone()
    };

    // Un mouvement rattaché à un compte inexistant fait échouer l'import : rien n'est appliqué.
    let batch = [
        tx(checking.id, "Courses"),
        tx(AccountId::generate(), "Orphelin"),
    ];
    assert_eq!(
        db.store.import(&updated, &batch).await,
        Err(RepoError::Reference)
    );
    assert_eq!(count(&db, "transactions").await, 0);
    assert_eq!(
        db.store.load().await.unwrap().accounts,
        vec![checking.clone()]
    );

    // Un compte invalide (détenteur inconnu) n'insère aucun mouvement.
    let orphan = Account {
        owners: Ownership::sole(MemberId::generate()),
        ..updated.clone()
    };
    assert_eq!(
        db.store
            .import(&orphan, &[tx(checking.id, "Courses")])
            .await,
        Err(RepoError::Reference)
    );
    assert_eq!(count(&db, "transactions").await, 0);
    assert_eq!(db.store.load().await.unwrap().accounts, vec![checking]);

    // Sinon, le compte et les mouvements sont enregistrés ensemble.
    assert_eq!(
        db.store
            .import(&updated, &[tx(updated.id, "Courses")])
            .await
            .unwrap(),
        1
    );
    assert_eq!(db.store.load().await.unwrap().accounts, vec![updated]);
}

fn totals_of(net: f64) -> Totals {
    Totals {
        gross_assets: net + 100.0,
        liabilities: 100.0,
        net_worth: net,
    }
}

/// Instantané sans détail par compte.
fn bare(net: f64) -> Snapshot {
    Snapshot {
        totals: totals_of(net),
        accounts: vec![],
    }
}

fn line(name: &str, kind: AccountKind, value: f64, gain: Option<f64>) -> AccountSnapshot {
    AccountSnapshot {
        account_id: AccountId::generate(),
        name: name.into(),
        kind,
        value,
        gain,
    }
}

#[tokio::test]
async fn snapshots_are_upserted_per_day_and_view() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let (day1, day2) = (date(2026, 1, 1), date(2026, 1, 2));
    db.store.upsert(day1, None, &bare(1000.0)).await.unwrap();
    db.store.upsert(day1, None, &bare(1100.0)).await.unwrap();
    db.store
        .upsert(day1, Some(alice.id), &bare(600.0))
        .await
        .unwrap();
    db.store.upsert(day2, None, &bare(1200.0)).await.unwrap();
    assert_eq!(count(&db, "snapshots").await, 3);

    let household = db.store.since(None, date(1900, 1, 1)).await.unwrap();
    assert_eq!(
        household
            .iter()
            .map(|p| (p.date, p.totals.net_worth))
            .collect::<Vec<_>>(),
        vec![(day1, 1100.0), (day2, 1200.0)]
    );
    assert_eq!(household[0].totals, totals_of(1100.0));
    // Un instantané enregistré sans compte reste détaillé : son détail est vide, pas absent.
    assert_eq!(household[0].accounts, Some(vec![]));
    assert_eq!(db.store.since(None, day2).await.unwrap().len(), 1);
    let member = db.store.since(Some(alice.id), day1).await.unwrap();
    assert_eq!(member.len(), 1);
    assert_eq!(member[0].totals.net_worth, 600.0);
    assert!(
        db.store
            .since(Some(MemberId::generate()), day1)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn snapshot_detail_is_kept_per_view_and_replaced_within_the_day() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let (day1, day2) = (date(2026, 1, 1), date(2026, 1, 2));
    let pea = line("PEA", AccountKind::Investment, 2000.0, Some(150.0));
    let loan = line("Prêt", AccountKind::Loan, 100.0, None);
    let morning = Snapshot {
        totals: totals_of(1900.0),
        accounts: vec![pea.clone(), loan.clone()],
    };
    db.store.upsert(day1, None, &morning).await.unwrap();
    // Le prêt est soldé dans la journée : le second relevé remplace tout le détail du jour.
    let evening = Snapshot {
        totals: totals_of(2000.0),
        accounts: vec![pea.clone()],
    };
    db.store.upsert(day1, None, &evening).await.unwrap();
    let alice_view = Snapshot {
        totals: totals_of(1000.0),
        accounts: vec![AccountSnapshot {
            value: 1000.0,
            gain: Some(75.0),
            ..pea.clone()
        }],
    };
    db.store
        .upsert(day1, Some(alice.id), &alice_view)
        .await
        .unwrap();
    db.store.upsert(day2, None, &morning).await.unwrap();

    let household = db.store.since(None, day1).await.unwrap();
    assert_eq!(household[0].accounts, Some(vec![pea.clone()]));
    // Trié par nom : « PEA » avant « Prêt ».
    assert_eq!(household[1].accounts, Some(vec![pea, loan]));
    let member = db.store.since(Some(alice.id), day1).await.unwrap();
    assert_eq!(member[0].accounts, Some(alice_view.accounts));
    assert_eq!(count(&db, "snapshot_accounts").await, 4);

    // Le détail d'un membre disparaît avec lui.
    db.store.delete_member(alice.id).await.unwrap();
    assert_eq!(count(&db, "snapshot_accounts").await, 3);
}

#[tokio::test]
async fn snapshots_recorded_before_the_detail_have_no_account_breakdown() {
    let db = test_db().await;
    // Instantané écrit tel qu'avant la migration 0004 : sans `detailed` ni lignes de comptes.
    sqlx::query(
        "INSERT INTO snapshots (date, member_id, gross_assets, liabilities, net_worth) \
         VALUES ('2026-01-01', NULL, 1100, 100, 1000)",
    )
    .execute(&db.pool)
    .await
    .unwrap();
    let points = db.store.since(None, date(1900, 1, 1)).await.unwrap();
    assert_eq!(points.len(), 1);
    assert_eq!(points[0].totals, totals_of(1000.0));
    assert_eq!(points[0].accounts, None);
}

#[tokio::test]
async fn correcting_a_snapshot_replaces_its_totals_and_drops_its_detail_in_one_view() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let day = date(2026, 1, 1);
    let detailed = Snapshot {
        totals: totals_of(1900.0),
        accounts: vec![line("PEA", AccountKind::Investment, 2000.0, None)],
    };
    db.store.upsert(day, None, &detailed).await.unwrap();
    db.store
        .upsert(day, Some(alice.id), &detailed)
        .await
        .unwrap();
    let fixed = totals_of(1500.0);

    assert!(db.store.correct(day, None, fixed).await.unwrap());
    assert!(
        !db.store
            .correct(date(2026, 1, 2), None, fixed)
            .await
            .unwrap()
    );
    let household = db.store.since(None, day).await.unwrap();
    assert_eq!(household[0].totals, fixed);
    assert_eq!(household[0].accounts, None);
    assert!(household[0].corrected);
    // La vue d'Alice garde son détail.
    let member = db.store.since(Some(alice.id), day).await.unwrap();
    assert!(!member[0].corrected);
    assert_eq!(member[0].accounts.as_ref().map(Vec::len), Some(1));
    assert_eq!(count(&db, "snapshot_accounts").await, 1);

    // Un nouvel instantané du même jour remplace la correction.
    db.store.upsert(day, None, &detailed).await.unwrap();
    let household = db.store.since(None, day).await.unwrap();
    assert!(!household[0].corrected);
    assert_eq!(household[0].totals, totals_of(1900.0));
}

#[tokio::test]
async fn snapshots_are_deleted_by_range_for_every_view_then_all_at_once() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let pea = line("PEA", AccountKind::Investment, 2000.0, None);
    for day in 1..=4 {
        let snapshot = Snapshot {
            totals: totals_of(f64::from(day) * 100.0),
            accounts: vec![pea.clone()],
        };
        db.store
            .upsert(date(2026, 1, day), None, &snapshot)
            .await
            .unwrap();
        db.store
            .upsert(date(2026, 1, day), Some(alice.id), &snapshot)
            .await
            .unwrap();
    }
    let range = DateRange::new(date(2026, 1, 2), date(2026, 1, 3)).unwrap();
    assert_eq!(db.store.delete_range(range).await.unwrap(), 2);
    assert_eq!(db.store.delete_range(range).await.unwrap(), 0);
    let days = |points: Vec<fineme_application::ports::SnapshotPoint>| {
        points.iter().map(|p| p.date).collect::<Vec<_>>()
    };
    let kept = vec![date(2026, 1, 1), date(2026, 1, 4)];
    assert_eq!(
        days(db.store.since(None, date(1900, 1, 1)).await.unwrap()),
        kept
    );
    assert_eq!(
        days(
            db.store
                .since(Some(alice.id), date(1900, 1, 1))
                .await
                .unwrap()
        ),
        kept
    );
    assert_eq!(count(&db, "snapshot_accounts").await, 4);

    assert_eq!(db.store.delete_all().await.unwrap(), 2);
    assert_eq!(count(&db, "snapshots").await, 0);
    assert_eq!(count(&db, "snapshot_accounts").await, 0);
    assert_eq!(db.store.delete_all().await.unwrap(), 0);
}

#[tokio::test]
async fn goals_keep_their_links_until_the_linked_account_disappears() {
    let db = test_db().await;
    let (alice, bob) = household(&db).await;
    let savings = account(AccountKind::Savings, "Livret", &[(alice.id, 1.0)]);
    save(&db, &savings).await;
    let goal = Goal {
        id: GoalId::generate(),
        name: "Voyage".into(),
        icon: Some("✈".into()),
        kind: GoalKind::Project,
        target_amount: Some(20_000.0),
        target_date: Some(date(2028, 1, 1)),
        monthly_contribution: Some(200.0),
        expected_return_pct: Some(5.0),
        monthly_expenses: None,
        coverage_months: None,
        account_ids: vec![savings.id],
        member_ids: vec![alice.id],
    };
    GoalRepository::save(&db.store, &goal).await.unwrap();
    assert_eq!(
        GoalRepository::list(&db.store).await.unwrap(),
        vec![goal.clone()]
    );

    let changed = Goal {
        name: "Tour du monde".into(),
        icon: None,
        target_date: None,
        member_ids: vec![bob.id],
        ..goal.clone()
    };
    GoalRepository::save(&db.store, &changed).await.unwrap();
    assert_eq!(
        GoalRepository::list(&db.store).await.unwrap(),
        vec![changed.clone()]
    );

    db.store.delete_account(savings.id).await.unwrap();
    assert!(
        GoalRepository::list(&db.store).await.unwrap()[0]
            .account_ids
            .is_empty()
    );
    assert!(GoalRepository::delete(&db.store, goal.id).await.unwrap());
    assert!(!GoalRepository::delete(&db.store, goal.id).await.unwrap());
    assert_eq!(count(&db, "goal_members").await, 0);
}

#[tokio::test]
async fn fx_rates_merge_and_connections_round_trip() {
    let db = test_db().await;
    let cur = |code: &str| Currency::parse(code).unwrap();
    db.store
        .save_rates(
            &FxRates::new([(cur("USD"), 1.10), (cur("GBP"), 0.85)]),
            at(1),
        )
        .await
        .unwrap();
    db.store
        .save_rates(&FxRates::new([(cur("USD"), 1.25)]), at(2))
        .await
        .unwrap();
    let (rates, updated_at) = db.store.rates().await.unwrap();
    assert_eq!(
        (rates.rate(&cur("USD")), rates.rate(&cur("GBP"))),
        (Some(1.25), Some(0.85))
    );
    assert_eq!(updated_at, Some(at(2)));

    let connection = Connection {
        id: ConnectionId::generate(),
        provider: "demo".into(),
        institution: Some("Banque Démo".into()),
        status: ConnectionStatus::Active,
        last_sync_at: None,
        error: None,
        external_id: Some("connexion-1".into()),
    };
    ConnectionRepository::save(&db.store, &connection)
        .await
        .unwrap();
    let failed = Connection {
        status: ConnectionStatus::Error,
        error: Some("jeton expiré".into()),
        last_sync_at: Some(at(3)),
        ..connection.clone()
    };
    ConnectionRepository::save(&db.store, &failed)
        .await
        .unwrap();
    assert_eq!(
        db.store.find(connection.id).await.unwrap(),
        Some(failed.clone())
    );
    assert_eq!(
        ConnectionRepository::list(&db.store).await.unwrap(),
        vec![failed]
    );
    assert_eq!(db.store.find(ConnectionId::generate()).await.unwrap(), None);

    // Un compte lié survit à la suppression de sa connexion.
    let alice = member("Alice", true);
    db.store.insert_member(&alice).await.unwrap();
    let mut linked = account(AccountKind::Checking, "Agrégé", &[(alice.id, 1.0)]);
    linked.connection_id = Some(connection.id);
    save(&db, &linked).await;
    assert!(
        ConnectionRepository::delete(&db.store, connection.id)
            .await
            .unwrap()
    );
    assert!(
        !ConnectionRepository::delete(&db.store, connection.id)
            .await
            .unwrap()
    );
    assert_eq!(
        db.store.load().await.unwrap().accounts[0].connection_id,
        None
    );
}

#[tokio::test]
async fn import_batch_is_applied_atomically() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let chloe = member("Chloé", false);
    let pea = account(
        AccountKind::Investment,
        "PEA",
        &[(alice.id, 0.5), (chloe.id, 0.5)],
    );
    let goal = Goal {
        id: GoalId::generate(),
        name: "Matelas".into(),
        icon: None,
        kind: GoalKind::SafetyNet,
        target_amount: None,
        target_date: None,
        monthly_contribution: None,
        expected_return_pct: None,
        monthly_expenses: Some(2000.0),
        coverage_months: Some(6.0),
        account_ids: vec![pea.id],
        member_ids: vec![chloe.id],
    };
    let recolored = Member {
        color: "#abcdef".into(),
        ..alice.clone()
    };
    let batch = ImportBatch {
        members: vec![recolored.clone(), chloe.clone()],
        accounts: vec![AccountWrite {
            account: pea.clone(),
            holdings: Some(vec![holding(pea.id, "ETF")]),
        }],
        goals: vec![goal.clone()],
    };
    db.store.apply(&batch).await.unwrap();
    // Rejouer le même lot ne duplique rien.
    db.store.apply(&batch).await.unwrap();
    let data = db.store.load().await.unwrap();
    assert_eq!(data.members.len(), 3);
    assert_eq!(data.members[0], recolored);
    assert_eq!(data.accounts, vec![pea.clone()]);
    assert_eq!(data.holdings.len(), 1);
    assert_eq!(
        GoalRepository::list(&db.store).await.unwrap(),
        vec![goal.clone()]
    );

    // Un lot invalide (objectif lié à un compte inexistant) n'applique rien, pas même ses membres.
    let broken = ImportBatch {
        members: vec![member("Dana", false)],
        accounts: vec![AccountWrite::account_only(account(
            AccountKind::Savings,
            "Livret",
            &[(alice.id, 1.0)],
        ))],
        goals: vec![Goal {
            id: GoalId::generate(),
            account_ids: vec![AccountId::generate()],
            ..goal
        }],
    };
    assert_eq!(db.store.apply(&broken).await, Err(RepoError::Reference));
    let data = db.store.load().await.unwrap();
    assert_eq!((data.members.len(), data.accounts.len()), (3, 1));
}

fn operation(account: AccountId, day: u32, movement: Movement) -> Operation {
    Operation {
        id: OperationId::generate(),
        account_id: account,
        date: date(2025, 6, day),
        movement,
        currency: Currency::eur(),
        note: Some("Note fictive".into()),
        recorded_at: at(day),
    }
}

#[tokio::test]
async fn operations_round_trip_update_positions_and_follow_their_holding() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let pea = account(AccountKind::Investment, "PEA", &[(alice.id, 1.0)]);
    save(&db, &pea).await;
    let etf = holding(pea.id, "ETF");
    db.store.save_holding(&etf, at(1)).await.unwrap();

    let buy = operation(
        pea.id,
        2,
        Movement::Buy(Trade::new(etf.id, 3.0, 100.0, 1.5).unwrap()),
    );
    let deposit = operation(
        pea.id,
        1,
        Movement::new(
            OperationKind::Deposit,
            MovementFields {
                amount: Some(500.0),
                ..MovementFields::default()
            },
        )
        .unwrap(),
    );
    let dividend = operation(
        pea.id,
        3,
        Movement::Dividend {
            holding: Some(etf.id),
            amount: Amount::new(4.2, "amount").unwrap(),
        },
    );
    let update = PositionUpdate {
        holding: etf.id,
        quantity: 3.0,
        buy_price: Some(100.5),
    };
    db.store
        .apply_operation(&OperationChange::Save(buy.clone()), &[update], at(5))
        .await
        .unwrap();
    for op in [&deposit, &dividend] {
        db.store
            .apply_operation(&OperationChange::Save(op.clone()), &[], at(5))
            .await
            .unwrap();
    }

    // Relecture fidèle, dans l'ordre chronologique.
    let listed = db.store.operations(pea.id).await.unwrap();
    assert_eq!(listed, vec![deposit.clone(), buy.clone(), dividend.clone()]);
    assert_eq!(
        db.store.find_operation(buy.id).await.unwrap(),
        Some(buy.clone())
    );
    let data = db.store.load().await.unwrap();
    let stored = data.holdings.iter().find(|h| h.id == etf.id).unwrap();
    assert_eq!((stored.quantity, stored.buy_price), (3.0, Some(100.5)));
    assert_eq!(stored.current_price, etf.current_price);
    assert_eq!(data.accounts[0].updated_at, at(5));

    // Remplacement puis suppression.
    let mut edited = buy.clone();
    edited.movement = Movement::Buy(Trade::new(etf.id, 4.0, 90.0, 0.0).unwrap());
    db.store
        .apply_operation(&OperationChange::Save(edited.clone()), &[], at(6))
        .await
        .unwrap();
    assert_eq!(db.store.find_operation(buy.id).await.unwrap(), Some(edited));
    db.store
        .apply_operation(
            &OperationChange::Delete {
                id: deposit.id,
                account: pea.id,
            },
            &[],
            at(6),
        )
        .await
        .unwrap();
    assert_eq!(count(&db, "investment_operations").await, 2);

    // Une opération vers une position inconnue est refusée, et rien n'est écrit.
    let orphan = operation(
        pea.id,
        4,
        Movement::Buy(Trade::new(HoldingId::generate(), 1.0, 1.0, 0.0).unwrap()),
    );
    assert_eq!(
        db.store
            .apply_operation(&OperationChange::Save(orphan), &[update], at(7))
            .await,
        Err(RepoError::Reference)
    );
    assert_eq!(count(&db, "investment_operations").await, 2);

    // Supprimer la position supprime son journal ; supprimer le compte, le reste.
    db.store.delete_holding(etf.id, at(8)).await.unwrap();
    assert_eq!(count(&db, "investment_operations").await, 0);
    let fee = operation(
        pea.id,
        5,
        Movement::Fee {
            holding: None,
            amount: Amount::new(2.0, "amount").unwrap(),
        },
    );
    db.store
        .apply_operation(&OperationChange::Save(fee), &[], at(8))
        .await
        .unwrap();
    db.store.delete_account(pea.id).await.unwrap();
    assert_eq!(count(&db, "investment_operations").await, 0);
}

#[tokio::test]
async fn operation_import_creates_holdings_and_operations_all_or_nothing() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let pea = account(AccountKind::Investment, "PEA", &[(alice.id, 1.0)]);
    save(&db, &pea).await;
    let new_etf = Holding {
        quantity: 0.0,
        buy_price: None,
        ..holding(pea.id, "ETF importé")
    };
    let buy = operation(
        pea.id,
        2,
        Movement::Buy(Trade::new(new_etf.id, 2.0, 50.0, 1.0).unwrap()),
    );
    let deposit = operation(
        pea.id,
        1,
        Movement::Deposit(Amount::new(100.0, "amount").unwrap()),
    );
    let update = PositionUpdate {
        holding: new_etf.id,
        quantity: 2.0,
        buy_price: Some(50.5),
    };
    let change = OperationChange::Import {
        account: pea.id,
        holdings: vec![new_etf.clone()],
        operations: vec![deposit.clone(), buy.clone()],
    };
    db.store
        .apply_operation(&change, &[update], at(4))
        .await
        .unwrap();
    assert_eq!(
        db.store.operations(pea.id).await.unwrap(),
        vec![deposit, buy]
    );
    let data = db.store.load().await.unwrap();
    let stored = data.holdings.iter().find(|h| h.id == new_etf.id).unwrap();
    assert_eq!((stored.quantity, stored.buy_price), (2.0, Some(50.5)));
    assert_eq!(data.accounts[0].updated_at, at(4));

    // Une opération orpheline annule tout l'import, positions créées comprises.
    let other = holding(pea.id, "Autre ETF");
    let orphan = operation(
        pea.id,
        3,
        Movement::Buy(Trade::new(HoldingId::generate(), 1.0, 1.0, 0.0).unwrap()),
    );
    let refused = OperationChange::Import {
        account: pea.id,
        holdings: vec![other],
        operations: vec![orphan],
    };
    assert_eq!(
        db.store.apply_operation(&refused, &[], at(5)).await,
        Err(RepoError::Reference)
    );
    assert_eq!(count(&db, "holdings").await, 1);
    assert_eq!(count(&db, "investment_operations").await, 2);
}

#[tokio::test]
async fn import_profiles_are_upserted_per_kind_and_deleted_with_their_account() {
    let db = test_db().await;
    let (alice, _) = household(&db).await;
    let checking = account(AccountKind::Checking, "Courant", &[(alice.id, 1.0)]);
    save(&db, &checking).await;
    let profile = |template: Option<&str>, header: &str, day: u32| {
        ImportProfile::new(
            checking.id,
            ImportKind::Transactions,
            template.map(Into::into),
            [
                ("date".to_string(), "Date opération".to_string()),
                ("label".to_string(), header.to_string()),
            ],
            at(day),
        )
        .unwrap()
    };
    db.store
        .save_profile(&profile(None, "Libellé", 1))
        .await
        .unwrap();
    let replaced = profile(Some("banque-generique"), "Libellé simplifié", 2);
    db.store.save_profile(&replaced).await.unwrap();
    let holdings = ImportProfile::new(
        checking.id,
        ImportKind::Holdings,
        None,
        [("name".to_string(), "Valeur".to_string())],
        at(3),
    )
    .unwrap();
    db.store.save_profile(&holdings).await.unwrap();
    assert_eq!(
        db.store.profiles(checking.id).await.unwrap(),
        vec![holdings, replaced]
    );
    assert!(
        db.store
            .profiles(AccountId::generate())
            .await
            .unwrap()
            .is_empty()
    );
    db.store.delete_account(checking.id).await.unwrap();
    assert_eq!(count(&db, "import_profiles").await, 0);
}
