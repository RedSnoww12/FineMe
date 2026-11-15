use chrono::{DateTime, Utc};

use crate::ids::ConnectionId;
use crate::property::string_enum;

string_enum!(ConnectionStatus { Active => "active", Error => "error", Expired => "expired" }
    invalid = "Statut de connexion invalide");

/// Connexion à un agrégateur bancaire.
#[derive(Debug, Clone, PartialEq)]
pub struct Connection {
    pub id: ConnectionId,
    pub provider: String,
    pub institution: Option<String>,
    pub status: ConnectionStatus,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
    /// Identifiant de la connexion côté fournisseur.
    pub external_id: Option<String>,
}
