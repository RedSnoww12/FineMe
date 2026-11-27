//! Ports sortants : ce dont les cas d'usage ont besoin, implémenté par les adaptateurs (et par les faux de test).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use fineme_domain::access_token::{AccessToken, McpCall, SecretDigest};
use fineme_domain::account::Account;
use fineme_domain::alert::{Alert, AlertRule};
use fineme_domain::budget::{BudgetTarget, CategoryRule};
use fineme_domain::connection::Connection;
use fineme_domain::fx::{Currency, FxRates};
use fineme_domain::goal::Goal;
use fineme_domain::history::DateRange;
use fineme_domain::holding::{Holding, HoldingKind};
use fineme_domain::ids::{
    AccessTokenId, AccountId, AlertId, BudgetRuleId, ConnectionId, GoalId, HoldingId, MemberId,
    OAuthGrantId, OperationId, TransactionId, UserId,
};
use fineme_domain::import_profile::ImportProfile;
use fineme_domain::login::LoginEvent;
use fineme_domain::member::{Member, User};
use fineme_domain::oauth::{AuthorizationRequest, ClientId, ClientMetadata, OAuthGrant};
use fineme_domain::operation::Operation;
use fineme_domain::performance::Benchmark;
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
    /// Contenu du jeton s'il est authentique et non expiré à `now`. Un jeton de défi est refusé.
    fn verify(&self, token: &str, now: DateTime<Utc>) -> Option<TokenSubject>;
    /// Jeton court de défi (seconde étape de connexion) : ne vaut pas session.
    fn issue_challenge(
        &self,
        subject: TokenSubject,
        now: DateTime<Utc>,
    ) -> Result<String, ProviderError>;
    /// Contenu d'un jeton de défi authentique et non expiré ; un jeton de session est refusé.
    fn verify_challenge(&self, token: &str, now: DateTime<Utc>) -> Option<TokenSubject>;
    /// Durée de validité d'un jeton de défi, en secondes.
    fn challenge_seconds(&self) -> i64;
}

/// Primitives du second facteur : aléa, HMAC-SHA1, chiffrement du secret au repos et QR code.
pub trait TotpSupport: Send + Sync {
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, ProviderError>;
    fn hmac_sha1(&self, key: &[u8], message: &[u8]) -> [u8; 20];
    /// Secret chiffré, prêt à stocker.
    fn seal(&self, secret: &[u8]) -> Result<String, ProviderError>;
    /// Secret déchiffré ; erreur si la valeur est altérée ou la clé a changé.
    fn open(&self, sealed: &str) -> Result<Vec<u8>, ProviderError>;
    /// Document SVG du QR code de `text`.
    fn qr_svg(&self, text: &str) -> Result<String, ProviderError>;
}

/// Primitives des jetons d'accès : aléa du secret et empreinte SHA-256 (seule forme conservée).
pub trait AccessTokenCrypto: Send + Sync {
    fn random_bytes(&self, len: usize) -> Result<Vec<u8>, ProviderError>;
    fn sha256(&self, data: &[u8]) -> [u8; 32];
}

// ---------------------------------------------------------------- persistance

#[derive(Debug, Clone, PartialEq)]
pub struct Credentials {
    pub user: User,
    pub password_hash: String,
    /// Version de session courante (0 à la création).
    pub session_version: i64,
    /// Incrémentée par les seuls changements de mot de passe (le second facteur n'y touche pas) : les
    /// autorisations OAuth s'y rattachent.
    pub password_version: i64,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn exists_any(&self) -> RepoResult<bool>;
    async fn find_by_email(&self, email: &str) -> RepoResult<Option<Credentials>>;
    async fn find_by_id(&self, id: UserId) -> RepoResult<Option<Credentials>>;
    /// L'utilisateur unique du foyer, s'il est configuré (consentement OAuth, sans session).
    async fn owner(&self) -> RepoResult<Option<Credentials>>;
    /// Remplace le hash du mot de passe, incrémente les versions de session et de mot de passe et révoque tous les jetons d'accès et
    /// autorisations OAuth actifs de l'utilisateur à `revoked_at`, tout ou rien. Renvoie la nouvelle version, ou `None` si
    /// l'utilisateur n'existe pas.
    async fn update_password(
        &self,
        id: UserId,
        password_hash: &str,
        revoked_at: DateTime<Utc>,
    ) -> RepoResult<Option<i64>>;
    /// Crée le premier utilisateur et son membre principal, de façon atomique.
    /// Renvoie `false` sans rien créer si un utilisateur existe déjà.
    async fn create_first(
        &self,
        credentials: &Credentials,
        primary_member: &Member,
    ) -> RepoResult<bool>;
}

/// Code de secours non utilisé (haché).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCodeHash {
    pub id: i64,
    pub hash: String,
}

/// État du second facteur d'un utilisateur.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecondFactorRecord {
    /// Secret chiffré actif ; `None` = second facteur désactivé.
    pub secret: Option<String>,
    /// Secret chiffré en attente de confirmation par un premier code.
    pub pending_secret: Option<String>,
    /// Dernier pas TOTP accepté (anti-rejeu).
    pub last_step: Option<i64>,
    /// Codes de secours encore utilisables.
    pub recovery_codes: Vec<RecoveryCodeHash>,
}

#[async_trait]
pub trait SecondFactorRepository: Send + Sync {
    /// État du second facteur (vide si l'utilisateur n'existe pas).
    async fn second_factor(&self, user: UserId) -> RepoResult<SecondFactorRecord>;
    async fn save_pending_secret(&self, user: UserId, sealed: &str) -> RepoResult<()>;
    /// Active le secret, retient le pas utilisé, remplace les codes de secours, retire le secret en attente et
    /// incrémente la version de session, tout ou rien. Renvoie la nouvelle version (`None` : utilisateur absent).
    async fn enable_second_factor(
        &self,
        user: UserId,
        sealed: &str,
        step: i64,
        recovery_hashes: &[String],
    ) -> RepoResult<Option<i64>>;
    /// Efface secret, secret en attente et codes de secours, et incrémente la version de session.
    async fn disable_second_factor(&self, user: UserId) -> RepoResult<Option<i64>>;
    /// Retient `step` s'il est postérieur au dernier pas accepté ; `false` = rejeu (écriture conditionnelle).
    async fn use_totp_step(&self, user: UserId, step: i64) -> RepoResult<bool>;
    /// Consomme un code de secours ; `false` s'il était déjà utilisé.
    async fn use_recovery_code(&self, user: UserId, id: i64) -> RepoResult<bool>;
    async fn replace_recovery_codes(&self, user: UserId, hashes: &[String]) -> RepoResult<()>;
}

/// Journal des tentatives de connexion et de réinitialisation.
#[async_trait]
pub trait LoginJournal: Send + Sync {
    async fn record(&self, event: &LoginEvent) -> RepoResult<()>;
    /// Les `limit` tentatives les plus récentes, de la plus récente à la plus ancienne.
    async fn recent(&self, limit: i64) -> RepoResult<Vec<LoginEvent>>;
    /// Supprime les tentatives antérieures à `before` ; renvoie le nombre de lignes supprimées.
    async fn prune_before(&self, before: DateTime<Utc>) -> RepoResult<u64>;
}

/// Jetons d'accès personnels (empreinte du secret seulement).
#[async_trait]
pub trait AccessTokenRepository: Send + Sync {
    /// Enregistre un nouveau jeton ; `Conflict` si l'empreinte existe déjà.
    async fn insert(&self, token: &AccessToken, digest: &SecretDigest) -> RepoResult<()>;
    /// Jetons de l'utilisateur, révoqués compris, du plus récent au plus ancien.
    async fn list(&self, user: UserId) -> RepoResult<Vec<AccessToken>>;
    async fn find(&self, id: AccessTokenId) -> RepoResult<Option<AccessToken>>;
    /// Jeton **actif** dont l'empreinte est `digest`, avec l'empreinte stockée (comparée ensuite en temps constant).
    async fn find_active_by_digest(
        &self,
        digest: &SecretDigest,
    ) -> RepoResult<Option<(AccessToken, SecretDigest)>>;
    /// Révoque un jeton actif de `user` ; `false` s'il était déjà révoqué, appartient à un autre utilisateur
    /// ou n'existe pas (écriture conditionnelle).
    async fn revoke(&self, id: AccessTokenId, user: UserId, at: DateTime<Utc>) -> RepoResult<bool>;
    /// Retient la date de dernière utilisation.
    async fn touch(&self, id: AccessTokenId, at: DateTime<Utc>) -> RepoResult<()>;
}

/// Journal des appels de tools MCP (sans paramètres ni résultats).
#[async_trait]
pub trait McpCallJournal: Send + Sync {
    async fn record(&self, call: &McpCall) -> RepoResult<()>;
    /// Les `limit` appels les plus récents, du plus récent au plus ancien.
    async fn recent(&self, limit: i64) -> RepoResult<Vec<McpCall>>;
    /// Supprime les appels antérieurs à `before` ; renvoie le nombre de lignes supprimées.
    async fn prune_before(&self, before: DateTime<Utc>) -> RepoResult<u64>;
}

// ---------------------------------------------------------------- serveur d'autorisation OAuth

/// Document de métadonnées d'un client OAuth, lu à l'URL qui est son identifiant.
#[async_trait]
pub trait ClientMetadataFetcher: Send + Sync {
    async fn fetch(&self, client_id: &ClientId) -> Result<ClientMetadata, ProviderError>;
}

/// Demande en attente du consentement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRequest {
    pub request: AuthorizationRequest,
    pub expires_at: DateTime<Utc>,
}

/// Code d'autorisation émis après consentement, à échanger une seule fois.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationCode {
    pub request: AuthorizationRequest,
    /// Identifiant de l'autorisation qui sera créée à l'échange.
    pub grant_id: OAuthGrantId,
    pub user: UserId,
    pub password_version: i64,
    pub expires_at: DateTime<Utc>,
}

/// Présentation d'un code : la première, ou un rejeu (le code désigne l'autorisation à révoquer).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeUse {
    First(Box<AuthorizationCode>),
    Replayed(Box<AuthorizationCode>),
}

/// Empreintes et échéances d'un jeton d'accès et de son jeton de renouvellement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IssuedPair {
    pub access: SecretDigest,
    pub access_expires_at: DateTime<Utc>,
    pub refresh: SecretDigest,
    pub refresh_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessRecord {
    pub grant: OAuthGrant,
    pub digest: SecretDigest,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshRecord {
    pub grant: OAuthGrant,
    pub expires_at: DateTime<Utc>,
    /// Déjà échangé : le présenter à nouveau est un rejeu.
    pub used: bool,
}

/// Demandes, codes, autorisations et jetons OAuth (empreintes des secrets seulement).
#[async_trait]
pub trait OAuthRepository: Send + Sync {
    /// Enregistre une demande ; retire les demandes expirées à `now` et ne garde que les
    /// [`MAX_PENDING_REQUESTS`] plus récentes.
    async fn save_request(
        &self,
        digest: &SecretDigest,
        pending: &PendingRequest,
        now: DateTime<Utc>,
    ) -> RepoResult<()>;
    async fn find_request(&self, digest: &SecretDigest) -> RepoResult<Option<PendingRequest>>;
    /// Retire la demande ; `false` si elle n'existait plus (une seule décision par demande).
    async fn take_request(&self, digest: &SecretDigest) -> RepoResult<bool>;
    async fn save_code(&self, digest: &SecretDigest, code: &AuthorizationCode) -> RepoResult<()>;
    /// Marque le code comme utilisé, atomiquement, et retient un rejeu ; `None` si inconnu.
    async fn consume_code(
        &self,
        digest: &SecretDigest,
        now: DateTime<Utc>,
    ) -> RepoResult<Option<CodeUse>>;
    /// Crée l'autorisation et ses premiers jetons, et révoque (à `created_at`) les autorisations actives du
    /// même utilisateur pour le même client, tout ou rien. `false` sans rien écrire si le code qui la désigne a
    /// été rejoué entre-temps.
    async fn create_grant(&self, grant: &OAuthGrant, pair: &IssuedPair) -> RepoResult<bool>;
    /// Autorisations de l'utilisateur, révoquées comprises, de la plus récente à la plus ancienne.
    async fn grants(&self, user: UserId) -> RepoResult<Vec<OAuthGrant>>;
    /// Révoque une autorisation active ; `false` sinon (écriture conditionnelle).
    async fn revoke_grant(&self, id: OAuthGrantId, at: DateTime<Utc>) -> RepoResult<bool>;
    async fn touch_grant(&self, id: OAuthGrantId, at: DateTime<Utc>) -> RepoResult<()>;
    async fn find_access(&self, digest: &SecretDigest) -> RepoResult<Option<AccessRecord>>;
    async fn find_refresh(&self, digest: &SecretDigest) -> RepoResult<Option<RefreshRecord>>;
    /// Marque l'ancien jeton de renouvellement comme utilisé et enregistre la nouvelle paire, tout ou rien ;
    /// `false` sans rien écrire s'il l'était déjà.
    async fn rotate(
        &self,
        old_refresh: &SecretDigest,
        grant: OAuthGrantId,
        pair: &IssuedPair,
    ) -> RepoResult<bool>;
    /// Purge demandes, codes et jetons expirés à `now` ; renvoie le nombre de lignes supprimées.
    async fn prune_expired(&self, now: DateTime<Utc>) -> RepoResult<u64>;
}

/// Demandes d'autorisation en attente gardées au plus : la route est publique.
pub const MAX_PENDING_REQUESTS: usize = 100;

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
    /// Import CSV : crée les positions nouvelles puis ajoute les opérations, tout ou rien.
    Import {
        account: AccountId,
        holdings: Vec<Holding>,
        operations: Vec<Operation>,
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

/// Préférences d'import CSV (correspondance des colonnes) par compte et type de contenu.
#[async_trait]
pub trait ImportProfileRepository: Send + Sync {
    async fn profiles(&self, account: AccountId) -> RepoResult<Vec<ImportProfile>>;
    /// Crée ou remplace la préférence du compte pour ce type.
    async fn save_profile(&self, profile: &ImportProfile) -> RepoResult<()>;
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
    /// Instantané en vigueur à `date` : le dernier au plus tard ce jour-là ; `None` si l'historique commence après.
    async fn at(
        &self,
        member: Option<MemberId>,
        date: NaiveDate,
    ) -> RepoResult<Option<SnapshotPoint>>;
    /// Premier instantané de la vue ; `None` si son historique est vide.
    async fn first(&self, member: Option<MemberId>) -> RepoResult<Option<SnapshotPoint>>;
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

/// Indice de référence choisi (un seul) et ses cours relevés, dans la devise de l'indice.
#[async_trait]
pub trait BenchmarkRepository: Send + Sync {
    async fn benchmark(&self) -> RepoResult<Option<Benchmark>>;
    /// Crée ou remplace l'indice ; `reset_prices` efface d'abord les cours relevés, tout ou rien.
    async fn save_benchmark(&self, benchmark: &Benchmark, reset_prices: bool) -> RepoResult<()>;
    /// Supprime l'indice et ses cours ; `false` si aucun indice n'était choisi.
    async fn delete_benchmark(&self) -> RepoResult<bool>;
    /// Enregistre ou remplace le cours d'un jour.
    async fn record_benchmark_price(&self, date: NaiveDate, price: f64) -> RepoResult<()>;
    /// Cours depuis `from`, du plus ancien au plus récent, un par jour : les cours relevés, complétés
    /// par l'historique des positions de même ISIN ou ticker cotées dans la même devise.
    async fn benchmark_prices(&self, from: NaiveDate) -> RepoResult<Vec<PricePoint>>;
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

/// Alertes enregistrées (clé de condition unique) et configuration des règles.
#[async_trait]
pub trait AlertRepository: Send + Sync {
    /// Toutes les alertes, archivées et résolues comprises.
    async fn alerts(&self) -> RepoResult<Vec<Alert>>;
    async fn find_alert(&self, id: AlertId) -> RepoResult<Option<Alert>>;
    /// Crée ou met à jour les alertes, toutes ou aucune.
    async fn save_alerts(&self, alerts: &[Alert]) -> RepoResult<()>;
    /// Règles enregistrées (celles qui manquent prennent leur valeur par défaut).
    async fn rules(&self) -> RepoResult<Vec<AlertRule>>;
    /// Crée ou remplace la règle de son type.
    async fn save_rule(&self, rule: &AlertRule) -> RepoResult<()>;
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
