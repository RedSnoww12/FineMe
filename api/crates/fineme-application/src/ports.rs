//! Ports sortants : ce dont les cas d'usage ont besoin, implémenté par les adaptateurs (et par les faux de test).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use fineme_domain::account::Account;
use fineme_domain::budget::{BudgetTarget, CategoryRule};
use fineme_domain::connection::Connection;
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::goal::Goal;
use fineme_domain::history::DateRange;
use fineme_domain::holding::{Holding, HoldingKind};
use fineme_domain::ids::{
    AccountId, BudgetRuleId, ConnectionId, GoalId, HoldingId, MemberId, OperationId, TransactionId,
    UserId,
};
use fineme_domain::member::{Member, User};
use fineme_domain::operation::Operation;
use fineme_domain::portfolio::{AccountSnapshot, Snapshot, Totals};
use fineme_domain::transaction::Transaction;

// ---------------------------------------------------------------- erreurs

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum RepoError {
    /// Contrainte d'unicité violée.
    #[error("cet élément existe déjà")]
    Conflict,
    /// Élément référencé ailleurs, ou référence vers un élément inexistant.
    #[error("élément référencé ailleurs ou introuvable")]
    Reference,
    #[error("{0}")]
    Backend(String),
}

pub type RepoResult<T> = Result<T, RepoError>;

/// Échec d'un service externe (cours, change, banque), avec une raison lisible.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ProviderError(pub String);

// ---------------------------------------------------------------- temps et sécurité

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;

    fn today(&self) -> NaiveDate {
        self.now().date_naive()
    }
}

#[async_trait]
pub trait PasswordHasher: Send + Sync {
    async fn hash(&self, password: &str) -> Result<String, ProviderError>;
    async fn verify(&self, password: &str, hash: &str) -> bool;
}

/// Ce que porte un jeton de session : l'utilisateur et la version de session en vigueur à l'émission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSubject {
    pub user: UserId,
    /// Incrémentée à chaque changement de mot de passe : un jeton d'une version antérieure est refusé.
    pub session_version: i64,
}

pub trait TokenService: Send + Sync {
    fn issue(&self, subject: TokenSubject, now: DateTime<Utc>) -> Result<String, ProviderError>;
    /// Contenu du jeton s'il est authentique et non expiré à `now`.
    fn verify(&self, token: &str, now: DateTime<Utc>) -> Option<TokenSubject>;
}

// ---------------------------------------------------------------- persistance

#[derive(Debug, Clone, PartialEq)]
pub struct Credentials {
    pub user: User,
    pub password_hash: String,
    /// Version de session courante (0 à la création).
    pub session_version: i64,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn exists_any(&self) -> RepoResult<bool>;
    async fn find_by_email(&self, email: &str) -> RepoResult<Option<Credentials>>;
    async fn find_by_id(&self, id: UserId) -> RepoResult<Option<Credentials>>;
    /// Remplace le hash du mot de passe et incrémente la version de session, en une seule écriture.
    /// Renvoie la nouvelle version, ou `None` si l'utilisateur n'existe pas.
    async fn update_password(&self, id: UserId, password_hash: &str) -> RepoResult<Option<i64>>;
    /// Crée le premier utilisateur et son membre principal, de façon atomique.
    /// Renvoie `false` sans rien créer si un utilisateur existe déjà.
    async fn create_first(
        &self,
        credentials: &Credentials,
        primary_member: &Member,
    ) -> RepoResult<bool>;
}

#[derive(Debug, Clone, Default)]
pub struct PortfolioData {
    pub members: Vec<Member>,
    pub accounts: Vec<Account>,
    pub holdings: Vec<Holding>,
}

/// Écriture d'un compte (ligne, détenteurs, bien, prêt). `holdings: Some(..)` remplace l'ensemble de ses positions.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountWrite {
    pub account: Account,
    pub holdings: Option<Vec<Holding>>,
}

impl AccountWrite {
    pub fn account_only(account: Account) -> Self {
        Self {
            account,
            holdings: None,
        }
    }
}

#[async_trait]
pub trait PortfolioRepository: Send + Sync {
    async fn load(&self) -> RepoResult<PortfolioData>;
    async fn members(&self) -> RepoResult<Vec<Member>>;
    async fn insert_member(&self, member: &Member) -> RepoResult<()>;
    async fn update_member(&self, member: &Member) -> RepoResult<()>;
    async fn delete_member(&self, id: MemberId) -> RepoResult<()>;
    /// Crée ou met à jour les comptes, tous ou aucun.
    async fn save_accounts(&self, writes: &[AccountWrite]) -> RepoResult<()>;
    async fn delete_account(&self, id: AccountId) -> RepoResult<bool>;
    /// Crée ou met à jour une position et date la modification de son compte.
    async fn save_holding(&self, holding: &Holding, at: DateTime<Utc>) -> RepoResult<()>;
    async fn delete_holding(&self, id: HoldingId, at: DateTime<Utc>) -> RepoResult<bool>;
    /// Enregistre des cours récupérés automatiquement et les ajoute à l'historique des cours
    /// (un point par position et par jour de `at`, dans la devise de la position), tous ou aucun.
    async fn update_prices(&self, prices: &[(HoldingId, f64)], at: DateTime<Utc>)
    -> RepoResult<()>;
    /// Historique des cours d'une position depuis `from`, du plus ancien au plus récent.
    async fn price_history(
        &self,
        holding: HoldingId,
        from: NaiveDate,
    ) -> RepoResult<Vec<PricePoint>>;
}

/// Cours d'une position à une date, dans la devise de la position à cette date.
#[derive(Debug, Clone, PartialEq)]
pub struct PricePoint {
    pub date: NaiveDate,
    pub price: f64,
    pub currency: Currency,
}

#[async_trait]
pub trait FxRateRepository: Send + Sync {
    /// Taux connus et date de leur dernière mise à jour.
    async fn rates(&self) -> RepoResult<(FxRates, Option<DateTime<Utc>>)>;
    async fn save_rates(&self, rates: &FxRates, at: DateTime<Utc>) -> RepoResult<()>;
}

#[async_trait]
pub trait TransactionRepository: Send + Sync {
    /// Page de mouvements (du plus récent au plus ancien) et nombre total.
    async fn page(
        &self,
        account: AccountId,
        limit: i64,
        offset: i64,
    ) -> RepoResult<(Vec<Transaction>, u64)>;
    /// Enregistre le compte (sans ses positions) puis insère les mouvements qui ne sont pas des
    /// doublons, tout ou rien ; renvoie le nombre de mouvements réellement insérés.
    async fn import(&self, account: &Account, transactions: &[Transaction]) -> RepoResult<u64>;
    async fn delete(&self, id: TransactionId) -> RepoResult<bool>;
    /// Mouvements de tous les comptes entre deux dates incluses.
    async fn between(&self, from: NaiveDate, to: NaiveDate) -> RepoResult<Vec<Transaction>>;
    async fn by_id(&self, id: TransactionId) -> RepoResult<Option<Transaction>>;
    /// Enregistre les choix manuels d'un mouvement (catégorie, virement interne) et, le cas échéant, crée ou met
    /// à jour la règle de même motif (l'identifiant existant est conservé), tout ou rien.
    async fn annotate(
        &self,
        transaction: &Transaction,
        rule: Option<&CategoryRule>,
    ) -> RepoResult<()>;
    /// Catégories (importées ou manuelles) utilisées par au moins un mouvement.
    async fn categories(&self) -> RepoResult<Vec<String>>;
}

/// Règles de catégorisation et cibles mensuelles du budget.
#[async_trait]
pub trait BudgetRepository: Send + Sync {
    async fn rules(&self) -> RepoResult<Vec<CategoryRule>>;
    async fn delete_rule(&self, id: BudgetRuleId) -> RepoResult<bool>;
    async fn targets(&self) -> RepoResult<Vec<BudgetTarget>>;
    /// Crée ou remplace la cible de sa catégorie.
    async fn save_target(&self, target: &BudgetTarget) -> RepoResult<()>;
    async fn delete_target(&self, category: &str) -> RepoResult<bool>;
}

/// Modification du journal d'opérations d'un compte.
#[derive(Debug, Clone, PartialEq)]
pub enum OperationChange {
    /// Crée ou remplace l'opération.
    Save(Operation),
    Delete {
        id: OperationId,
        account: AccountId,
    },
}

/// Quantité et PRU d'une position recalculés à partir du journal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionUpdate {
    pub holding: HoldingId,
    pub quantity: f64,
    pub buy_price: Option<f64>,
}

#[async_trait]
pub trait OperationRepository: Send + Sync {
    /// Opérations d'un compte, dans un ordre quelconque.
    async fn operations(&self, account: AccountId) -> RepoResult<Vec<Operation>>;
    async fn find_operation(&self, id: OperationId) -> RepoResult<Option<Operation>>;
    /// Applique la modification du journal, écrit les positions recalculées et date la modification
    /// du compte, tout ou rien.
    async fn apply_operation(
        &self,
        change: &OperationChange,
        positions: &[PositionUpdate],
        at: DateTime<Utc>,
    ) -> RepoResult<()>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotPoint {
    pub date: NaiveDate,
    pub totals: Totals,
    /// Détail par compte ; `None` pour un instantané enregistré avant l'introduction du détail.
    pub accounts: Option<Vec<AccountSnapshot>>,
    /// Totaux corrigés à la main (le détail par compte a alors été retiré).
    pub corrected: bool,
}

#[async_trait]
pub trait SnapshotRepository: Send + Sync {
    /// Enregistre ou remplace l'instantané d'un jour pour une vue (`None` = foyer).
    async fn upsert(
        &self,
        date: NaiveDate,
        member: Option<MemberId>,
        snapshot: &Snapshot,
    ) -> RepoResult<()>;
    async fn since(
        &self,
        member: Option<MemberId>,
        from: NaiveDate,
    ) -> RepoResult<Vec<SnapshotPoint>>;
    /// Remplace les totaux d'un jour pour une vue, retire son détail par compte et le marque corrigé.
    /// Renvoie `false` si aucun instantané n'existe ce jour-là pour cette vue.
    async fn correct(
        &self,
        date: NaiveDate,
        member: Option<MemberId>,
        totals: Totals,
    ) -> RepoResult<bool>;
    /// Supprime les instantanés de la plage (toutes vues) ; renvoie le nombre de jours supprimés.
    async fn delete_range(&self, range: DateRange) -> RepoResult<u64>;
    /// Supprime tout l'historique (toutes vues) ; renvoie le nombre de jours supprimés.
    async fn delete_all(&self) -> RepoResult<u64>;
}

#[async_trait]
pub trait GoalRepository: Send + Sync {
    async fn list(&self) -> RepoResult<Vec<Goal>>;
    async fn save(&self, goal: &Goal) -> RepoResult<()>;
    async fn delete(&self, id: GoalId) -> RepoResult<bool>;
}

#[async_trait]
pub trait ConnectionRepository: Send + Sync {
    async fn list(&self) -> RepoResult<Vec<Connection>>;
    async fn find(&self, id: ConnectionId) -> RepoResult<Option<Connection>>;
    async fn save(&self, connection: &Connection) -> RepoResult<()>;
    async fn delete(&self, id: ConnectionId) -> RepoResult<bool>;
}

/// Résultat d'un import de bundle, à appliquer en une seule fois.
#[derive(Debug, Clone, Default)]
pub struct ImportBatch {
    pub members: Vec<Member>,
    pub accounts: Vec<AccountWrite>,
    pub goals: Vec<Goal>,
}

#[async_trait]
pub trait ImportRepository: Send + Sync {
    /// Applique tout le lot ou rien.
    async fn apply(&self, batch: &ImportBatch) -> RepoResult<()>;
}

// ---------------------------------------------------------------- services externes

#[derive(Debug, Clone, PartialEq)]
pub struct QuoteRequest {
    pub holding_id: HoldingId,
    pub isin: Option<String>,
    pub symbol: Option<String>,
    /// Devise dans laquelle la position est tenue.
    pub currency: Currency,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Quote {
    pub price: f64,
    pub currency: Currency,
}

/// Source de cours pour un ou plusieurs types de positions (stratégie par fournisseur).
#[async_trait]
pub trait PriceProvider: Send + Sync {
    fn handles(&self, kind: HoldingKind) -> bool;
    /// Un résultat par demande : le cours, ou la raison de l'échec. Ne doit jamais paniquer ni bloquer longtemps.
    async fn quotes(
        &self,
        requests: &[QuoteRequest],
    ) -> HashMap<HoldingId, Result<Quote, ProviderError>>;
}

#[async_trait]
pub trait FxProvider: Send + Sync {
    /// Taux de change, en unités de devise pour 1 EUR.
    async fn eur_rates(&self) -> Result<Vec<(String, f64)>, ProviderError>;
}

/// Connexion initialisée chez un agrégateur bancaire.
#[derive(Debug, Clone, PartialEq)]
pub struct NewBankConnection {
    pub external_id: String,
    /// URL vers laquelle rediriger l'utilisateur pour autoriser l'accès.
    pub redirect_url: String,
    pub institution: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BankTransaction {
    pub date: NaiveDate,
    pub label: String,
    pub amount: f64,
    pub category: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BankAccount {
    pub external_id: String,
    pub name: String,
    pub institution: Option<String>,
    /// Un `AccountKind` (checking, savings…).
    pub kind: String,
    pub currency: String,
    pub balance: f64,
    pub transactions: Vec<BankTransaction>,
}

/// Contrat à implémenter pour brancher un agrégateur bancaire (aucun n'est fourni par défaut).
#[async_trait]
pub trait BankProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    /// Vrai si les identifiants du fournisseur sont présents.
    fn configured(&self) -> bool;
    async fn create_connection(&self) -> Result<NewBankConnection, ProviderError>;
    async fn fetch_accounts(
        &self,
        connection_external_id: &str,
    ) -> Result<Vec<BankAccount>, ProviderError>;
}
