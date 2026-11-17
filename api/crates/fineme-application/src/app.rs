//! Assemblage des cas d'usage à partir des ports (appelé par la composition root et par les tests).

use std::sync::Arc;

use crate::accounts::AccountService;
use crate::auth::AuthService;
use crate::bundle::BundleService;
use crate::connections::ConnectionService;
use crate::error::AppResult;
use crate::goals::GoalService;
use crate::holdings::HoldingService;
use crate::members::MemberService;
use crate::overview::OverviewService;
use crate::ports::{
    BankProvider, Clock, ConnectionRepository, FxProvider, FxRateRepository, GoalRepository,
    ImportRepository, PasswordHasher, PortfolioRepository, PriceProvider, SnapshotRepository,
    TokenService, TransactionRepository, UserRepository,
};
use crate::prices::PriceService;
use crate::reader::PortfolioReader;
use crate::real_estate::RealEstateService;
use crate::transactions::TransactionService;

/// Tous les ports sortants dont l'application a besoin.
#[derive(Clone)]
pub struct Ports {
    pub users: Arc<dyn UserRepository>,
    pub portfolio: Arc<dyn PortfolioRepository>,
    pub fx_rates: Arc<dyn FxRateRepository>,
    pub transactions: Arc<dyn TransactionRepository>,
    pub snapshots: Arc<dyn SnapshotRepository>,
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
    pub transactions: TransactionService,
    pub overview: OverviewService,
    pub real_estate: RealEstateService,
    pub goals: GoalService,
    pub connections: ConnectionService,
    pub bundle: BundleService,
    pub prices: PriceService,
}

impl App {
    pub async fn build(ports: Ports, settings: Settings) -> AppResult<Self> {
        let reader = PortfolioReader {
            repo: ports.portfolio.clone(),
            fx: ports.fx_rates.clone(),
            clock: ports.clock.clone(),
        };
        // Hash factice calculé une fois : vérifié à la connexion quand l'e-mail est inconnu.
        let dummy_hash = ports.hasher.hash("mot-de-passe-factice-fineme").await?;
        Ok(Self {
            auth: AuthService {
                users: ports.users,
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
            },
            transactions: TransactionService {
                portfolio: ports.portfolio.clone(),
                transactions: ports.transactions.clone(),
                clock: ports.clock.clone(),
            },
            overview: OverviewService {
                reader: reader.clone(),
                members: ports.portfolio.clone(),
                snapshots: ports.snapshots,
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
        })
    }
}
