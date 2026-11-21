//! Assemblage des cas d'usage à partir des ports (appelé par la composition root et par les tests).

use std::sync::{Arc, Mutex};

use fineme_domain::throttle::LoginThrottle;

use crate::accounts::AccountService;
use crate::auth::AuthService;
use crate::budget::BudgetService;
use crate::bundle::BundleService;
use crate::connections::ConnectionService;
use crate::error::AppResult;
use crate::goals::GoalService;
use crate::history::HistoryService;
use crate::holdings::HoldingService;
use crate::members::MemberService;
use crate::operations::OperationService;
use crate::overview::OverviewService;
use crate::performance::PerformanceService;
use crate::ports::{
    BankProvider, BenchmarkRepository, BudgetRepository, Clock, ConnectionRepository, FxProvider,
    FxRateRepository, GoalRepository, ImportRepository, LoginJournal, OperationRepository,
    PasswordHasher, PortfolioRepository, PriceProvider, SnapshotRepository, TokenService,
    TransactionRepository, UserRepository,
};
use crate::prices::PriceService;
use crate::reader::PortfolioReader;
use crate::real_estate::RealEstateService;
use crate::transactions::TransactionService;

/// Tous les ports sortants dont l'application a besoin.
#[derive(Clone)]
pub struct Ports {
    pub users: Arc<dyn UserRepository>,
    pub login_journal: Arc<dyn LoginJournal>,
    pub portfolio: Arc<dyn PortfolioRepository>,
    pub fx_rates: Arc<dyn FxRateRepository>,
    pub transactions: Arc<dyn TransactionRepository>,
    pub budget: Arc<dyn BudgetRepository>,
    pub operations: Arc<dyn OperationRepository>,
    pub snapshots: Arc<dyn SnapshotRepository>,
    pub benchmarks: Arc<dyn BenchmarkRepository>,
    pub goals: Arc<dyn GoalRepository>,
    pub connections: Arc<dyn ConnectionRepository>,
    pub imports: Arc<dyn ImportRepository>,
    pub clock: Arc<dyn Clock>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub tokens: Arc<dyn TokenService>,
    pub fx_provider: Arc<dyn FxProvider>,
    pub price_providers: Vec<Arc<dyn PriceProvider>>,
    pub bank_providers: Vec<Arc<dyn BankProvider>>,
}

/// Réglages applicatifs issus de la configuration (lue par la composition root, jamais ici).
#[derive(Debug, Clone, Default)]
pub struct Settings {
    /// Code exigé pour la première configuration ; `None` ou vide = non exigé.
    pub setup_token: Option<String>,
    /// Code de secours pour réinitialiser un mot de passe perdu ; `None` ou vide = récupération désactivée.
    pub recovery_token: Option<String>,
    /// Secret exigé par la tâche quotidienne ; `None` ou vide = tâche désactivée.
    pub cron_secret: Option<String>,
}

/// Code facultatif : espaces retirés, vide = absent.
fn non_blank(code: Option<String>) -> Option<String> {
    code.map(|code| code.trim().to_string())
        .filter(|code| !code.is_empty())
}

/// Les cas d'usage, regroupés par domaine fonctionnel.
pub struct App {
    pub auth: AuthService,
    pub members: MemberService,
    pub accounts: AccountService,
    pub holdings: HoldingService,
    pub operations: OperationService,
    pub transactions: TransactionService,
    pub budget: BudgetService,
    pub overview: OverviewService,
    pub history: HistoryService,
    pub performance: PerformanceService,
    pub real_estate: RealEstateService,
    pub goals: GoalService,
    pub connections: ConnectionService,
    pub bundle: BundleService,
    pub prices: PriceService,
    /// Secret de la tâche quotidienne (voir `daily`).
    pub(crate) cron_secret: Option<String>,
}

impl App {
    pub async fn build(ports: Ports, settings: Settings) -> AppResult<Self> {
        let reader = PortfolioReader {
            repo: ports.portfolio.clone(),
            fx: ports.fx_rates.clone(),
            clock: ports.clock.clone(),
        };
        let operations = OperationService {
            reader: reader.clone(),
            operations: ports.operations.clone(),
            clock: ports.clock.clone(),
        };
        // Hash factice calculé une fois : vérifié à la connexion quand l'e-mail est inconnu.
        let dummy_hash = ports.hasher.hash("mot-de-passe-factice-fineme").await?;
        Ok(Self {
            auth: AuthService {
                users: ports.users,
                journal: ports.login_journal,
                throttle: Mutex::new(LoginThrottle::default()),
                hasher: ports.hasher,
                tokens: ports.tokens,
                clock: ports.clock.clone(),
                dummy_hash,
                setup_token: non_blank(settings.setup_token),
                recovery_token: non_blank(settings.recovery_token),
            },
            members: MemberService {
                repo: ports.portfolio.clone(),
            },
            accounts: AccountService {
                reader: reader.clone(),
                repo: ports.portfolio.clone(),
                clock: ports.clock.clone(),
            },
            holdings: HoldingService {
                reader: reader.clone(),
                repo: ports.portfolio.clone(),
                clock: ports.clock.clone(),
                operations: operations.clone(),
            },
            operations,
            transactions: TransactionService {
                portfolio: ports.portfolio.clone(),
                transactions: ports.transactions.clone(),
                clock: ports.clock.clone(),
            },
            budget: BudgetService {
                reader: reader.clone(),
                transactions: ports.transactions.clone(),
                budget: ports.budget,
            },
            overview: OverviewService {
                reader: reader.clone(),
                members: ports.portfolio.clone(),
                snapshots: ports.snapshots.clone(),
                clock: ports.clock.clone(),
            },
            performance: PerformanceService {
                reader: reader.clone(),
                snapshots: ports.snapshots.clone(),
                operations: ports.operations.clone(),
                benchmarks: ports.benchmarks,
                fx_rates: ports.fx_rates.clone(),
                price_providers: ports.price_providers.clone(),
                clock: ports.clock.clone(),
            },
            history: HistoryService {
                snapshots: ports.snapshots,
                members: ports.portfolio.clone(),
                clock: ports.clock.clone(),
            },
            real_estate: RealEstateService {
                reader: reader.clone(),
            },
            goals: GoalService {
                reader: reader.clone(),
                goals: ports.goals.clone(),
            },
            connections: ConnectionService {
                connections: ports.connections,
                portfolio: ports.portfolio.clone(),
                transactions: ports.transactions,
                providers: ports.bank_providers,
                clock: ports.clock.clone(),
            },
            bundle: BundleService {
                reader,
                goals: ports.goals,
                imports: ports.imports,
                clock: ports.clock.clone(),
            },
            prices: PriceService {
                portfolio: ports.portfolio,
                fx_rates: ports.fx_rates,
                fx_provider: ports.fx_provider,
                price_providers: ports.price_providers,
                clock: ports.clock,
            },
            cron_secret: non_blank(settings.cron_secret),
        })
    }
}
