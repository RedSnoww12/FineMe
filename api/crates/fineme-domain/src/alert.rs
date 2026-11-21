//! Alertes et rappels : règles configurables évaluées sur une photographie du patrimoine, puis
//! rapprochées des alertes déjà enregistrées pour qu'une même condition ne crée qu'une alerte.
//!
//! Chaque condition a une clé stable (`goal_overdue:<id>`, `savings_ceiling:<id>`…). Tant que la condition
//! dure, l'alerte existante est conservée (lue ou archivée, elle le reste) ; quand elle disparaît, l'alerte est
//! marquée résolue ; si elle réapparaît plus tard, l'alerte est rouverte comme une nouvelle.

use chrono::{DateTime, NaiveDate, Utc};

use crate::connection::{Connection, ConnectionStatus};
use crate::error::{DomainError, finite};
use crate::goal::GoalStatus;
use crate::ids::{AccountId, AlertId, GoalId};
use crate::property::string_enum;

/// Seuil par défaut d'approche du plafond d'un livret, en % du plafond.
pub const DEFAULT_CEILING_THRESHOLD_PCT: f64 = 90.0;
/// Plafond par défaut d'un livret, en EUR (plafond réglementaire du Livret A), modifiable dans la règle.
pub const DEFAULT_SAVINGS_CEILING: f64 = 22_950.0;
/// Seuil par défaut de variation du patrimoine net entre deux instantanés, en %.
pub const DEFAULT_NET_WORTH_CHANGE_PCT: f64 = 10.0;
/// Seuil maximal de variation accepté (1 000 %).
const MAX_CHANGE_THRESHOLD_PCT: f64 = 1000.0;

string_enum!(AlertRuleKind {
    Goals => "goals", SavingsCeiling => "savings_ceiling", NetWorthChange => "net_worth_change",
    Connections => "connections", StalePrices => "stale_prices",
} invalid = "Règle d'alerte inconnue (goals, savings_ceiling, net_worth_change, connections, stale_prices)");

string_enum!(AlertKind {
    GoalReached => "goal_reached", GoalOverdue => "goal_overdue", SavingsCeiling => "savings_ceiling",
    NetWorthChange => "net_worth_change", ConnectionExpired => "connection_expired",
    ConnectionError => "connection_error", StalePrices => "stale_prices",
} invalid = "Type d'alerte invalide");

string_enum!(AlertSeverity { Info => "info", Warning => "warning" }
    invalid = "Gravité d'alerte invalide (info ou warning)");

impl AlertRuleKind {
    /// Seuil en pourcentage par défaut, pour les règles qui en ont un.
    fn default_threshold(self) -> Option<f64> {
        match self {
            AlertRuleKind::SavingsCeiling => Some(DEFAULT_CEILING_THRESHOLD_PCT),
            AlertRuleKind::NetWorthChange => Some(DEFAULT_NET_WORTH_CHANGE_PCT),
            _ => None,
        }
    }

    fn default_ceiling(self) -> Option<f64> {
        (self == AlertRuleKind::SavingsCeiling).then_some(DEFAULT_SAVINGS_CEILING)
    }
}

// ---------------------------------------------------------------- configuration des règles

/// Règle d'alerte : activée ou non, avec ses seuils. Un seuil n'existe que pour la règle qui l'utilise,
/// et il est toujours valide (construction contrôlée par [`AlertRule::new`]).
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRule {
    kind: AlertRuleKind,
    enabled: bool,
    threshold_pct: Option<f64>,
    ceiling: Option<f64>,
}

impl AlertRule {
    /// Règle validée : seuil requis pour `savings_ceiling` (]0, 100]) et `net_worth_change` (]0, 1 000]),
    /// plafond requis et strictement positif pour `savings_ceiling`, rien pour les autres règles.
    pub fn new(
        kind: AlertRuleKind,
        enabled: bool,
        threshold_pct: Option<f64>,
        ceiling: Option<f64>,
    ) -> Result<Self, DomainError> {
        let threshold_pct = match (kind.default_threshold(), threshold_pct) {
            (None, None) => None,
            (None, Some(_)) => return Err(DomainError::Rule("Cette règle n'a pas de seuil")),
            (Some(_), None) => return Err(DomainError::Rule("Le seuil de la règle est requis")),
            (Some(_), Some(value)) => Some(check_threshold(kind, value)?),
        };
        let ceiling = match (kind.default_ceiling(), ceiling) {
            (None, None) => None,
            (None, Some(_)) => return Err(DomainError::Rule("Cette règle n'a pas de plafond")),
            (Some(_), None) => return Err(DomainError::Rule("Le plafond du livret est requis")),
            (Some(_), Some(value)) => {
                if finite(value, "le plafond")? <= 0.0 {
                    return Err(DomainError::NotPositive("Le plafond"));
                }
                Some(value)
            }
        };
        Ok(Self {
            kind,
            enabled,
            threshold_pct,
            ceiling,
        })
    }

    /// Règle par défaut : activée, seuils par défaut.
    pub fn default_for(kind: AlertRuleKind) -> Self {
        Self {
            kind,
            enabled: true,
            threshold_pct: kind.default_threshold(),
            ceiling: kind.default_ceiling(),
        }
    }

    /// Mise à jour partielle (champ absent = inchangé), revalidée.
    pub fn updated(
        &self,
        enabled: Option<bool>,
        threshold_pct: Option<f64>,
        ceiling: Option<f64>,
    ) -> Result<Self, DomainError> {
        Self::new(
            self.kind,
            enabled.unwrap_or(self.enabled),
            threshold_pct.or(self.threshold_pct),
            ceiling.or(self.ceiling),
        )
    }

    pub fn kind(&self) -> AlertRuleKind {
        self.kind
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn threshold_pct(&self) -> Option<f64> {
        self.threshold_pct
    }
    pub fn ceiling(&self) -> Option<f64> {
        self.ceiling
    }
}

fn check_threshold(kind: AlertRuleKind, value: f64) -> Result<f64, DomainError> {
    let value = finite(value, "le seuil")?;
    let max = match kind {
        AlertRuleKind::SavingsCeiling => 100.0,
        _ => MAX_CHANGE_THRESHOLD_PCT,
    };
    if value <= 0.0 || value > max {
        return Err(DomainError::Rule(match kind {
            AlertRuleKind::SavingsCeiling => {
                "Le seuil du plafond doit être compris entre 0 (exclu) et 100 %"
            }
            _ => "Le seuil de variation doit être compris entre 0 (exclu) et 1 000 %",
        }));
    }
    Ok(value)
}

/// L'ensemble des règles, une par type : les règles jamais enregistrées prennent leur valeur par défaut.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertRules(Vec<AlertRule>);

impl AlertRules {
    pub fn with_defaults(stored: Vec<AlertRule>) -> Self {
        Self(
            AlertRuleKind::ALL
                .iter()
                .map(|kind| {
                    stored
                        .iter()
                        .find(|rule| rule.kind == *kind)
                        .cloned()
                        .unwrap_or_else(|| AlertRule::default_for(*kind))
                })
                .collect(),
        )
    }

    pub fn get(&self, kind: AlertRuleKind) -> AlertRule {
        self.0
            .iter()
            .find(|rule| rule.kind == kind)
            .cloned()
            .unwrap_or_else(|| AlertRule::default_for(kind))
    }

    pub fn all(&self) -> &[AlertRule] {
        &self.0
    }

    /// La règle si elle est activée.
    fn active(&self, kind: AlertRuleKind) -> Option<AlertRule> {
        Some(self.get(kind)).filter(AlertRule::enabled)
    }
}

// ---------------------------------------------------------------- évaluation (fonctions pures)

/// Condition détectée par une règle, pas encore rapprochée des alertes enregistrées.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertCandidate {
    /// Identifie la condition : deux évaluations qui la retrouvent produisent la même clé.
    pub key: String,
    pub kind: AlertKind,
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GoalState {
    pub id: GoalId,
    pub name: String,
    pub status: GoalStatus,
}

/// Solde d'un livret en EUR.
#[derive(Debug, Clone, PartialEq)]
pub struct SavingsBalance {
    pub account_id: AccountId,
    pub name: String,
    pub value: f64,
}

/// Patrimoine net du foyer à la date d'un instantané.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NetWorthPoint {
    pub date: NaiveDate,
    pub net_worth: f64,
}

/// Ce que les règles observent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AlertInputs {
    pub goals: Vec<GoalState>,
    pub savings: Vec<SavingsBalance>,
    /// Deux derniers instantanés du foyer (précédent, dernier), s'il y en a deux.
    pub net_worth: Option<(NetWorthPoint, NetWorthPoint)>,
    pub connections: Vec<Connection>,
    /// Noms des positions dont le cours automatique n'est plus mis à jour.
    pub stale_prices: Vec<String>,
}

/// Montant en euros arrondi, groupé par milliers (« 22 950 € »).
fn eur(amount: f64) -> String {
    let rounded = amount.round();
    let digits = format!("{:.0}", rounded.abs());
    let mut grouped = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push('\u{202f}');
        }
        grouped.push(digit);
    }
    let sign = if rounded < 0.0 { "-" } else { "" };
    format!("{sign}{grouped} €")
}

/// Objectif atteint (projet complété, épargne de précaution constituée) ou date cible dépassée.
pub fn goal_alerts(goals: &[GoalState]) -> Vec<AlertCandidate> {
    goals
        .iter()
        .filter_map(|goal| match goal.status {
            GoalStatus::Completed | GoalStatus::Funded => Some(AlertCandidate {
                key: format!("goal_reached:{}", goal.id),
                kind: AlertKind::GoalReached,
                severity: AlertSeverity::Info,
                title: format!("Objectif atteint : {}", goal.name),
                message: format!("Le montant cible de l'objectif « {} » est atteint.", goal.name),
            }),
            GoalStatus::Overdue => Some(AlertCandidate {
                key: format!("goal_overdue:{}", goal.id),
                kind: AlertKind::GoalOverdue,
                severity: AlertSeverity::Warning,
                title: format!("Objectif en retard : {}", goal.name),
                message: format!(
                    "La date cible de l'objectif « {} » est passée sans que le montant soit atteint.",
                    goal.name
                ),
            }),
            _ => None,
        })
        .collect()
}

/// Livret dont le solde atteint `threshold_pct` % du plafond (ou le dépasse).
pub fn savings_ceiling_alerts(
    savings: &[SavingsBalance],
    threshold_pct: f64,
    ceiling: f64,
) -> Vec<AlertCandidate> {
    if ceiling <= 0.0 {
        return Vec::new();
    }
    savings
        .iter()
        .filter(|account| account.value >= ceiling * threshold_pct / 100.0)
        .map(|account| {
            let pct = account.value / ceiling * 100.0;
            AlertCandidate {
                key: format!("savings_ceiling:{}", account.account_id),
                kind: AlertKind::SavingsCeiling,
                severity: AlertSeverity::Warning,
                title: format!("Plafond proche : {}", account.name),
                message: format!(
                    "« {} » atteint {pct:.0} % de son plafond ({} sur {}).",
                    account.name,
                    eur(account.value),
                    eur(ceiling)
                ),
            }
        })
        .collect()
}

/// Variation du patrimoine net d'au moins `threshold_pct` % entre deux instantanés consécutifs.
/// Pas d'alerte si le patrimoine précédent est nul (variation non définie).
pub fn net_worth_change_alert(
    points: Option<(NetWorthPoint, NetWorthPoint)>,
    threshold_pct: f64,
) -> Option<AlertCandidate> {
    let (previous, latest) = points?;
    if previous.net_worth == 0.0 {
        return None;
    }
    let delta = latest.net_worth - previous.net_worth;
    let pct = delta / previous.net_worth.abs() * 100.0;
    if pct.abs() < threshold_pct {
        return None;
    }
    let direction = if delta >= 0.0 { "hausse" } else { "baisse" };
    Some(AlertCandidate {
        key: format!("net_worth_change:{}", latest.date),
        kind: AlertKind::NetWorthChange,
        severity: AlertSeverity::Warning,
        title: format!("Forte {direction} du patrimoine ({pct:+.1} %)"),
        message: format!(
            "Le patrimoine net est passé de {} le {} à {} le {} ({}).",
            eur(previous.net_worth),
            previous.date.format("%d/%m/%Y"),
            eur(latest.net_worth),
            latest.date.format("%d/%m/%Y"),
            eur(delta)
        ),
    })
}

/// Connexion bancaire expirée ou en erreur.
pub fn connection_alerts(connections: &[Connection]) -> Vec<AlertCandidate> {
    connections
        .iter()
        .filter_map(|connection| {
            let label = connection
                .institution
                .clone()
                .unwrap_or_else(|| connection.provider.clone());
            let (kind, title, message) = match connection.status {
                ConnectionStatus::Active => return None,
                ConnectionStatus::Expired => (
                    AlertKind::ConnectionExpired,
                    format!("Connexion expirée : {label}"),
                    "L'accès à la banque a expiré : renouvelez l'autorisation pour reprendre la synchronisation."
                        .to_string(),
                ),
                ConnectionStatus::Error => (
                    AlertKind::ConnectionError,
                    format!("Connexion en erreur : {label}"),
                    connection
                        .error
                        .clone()
                        .unwrap_or_else(|| "La dernière synchronisation a échoué.".to_string()),
                ),
            };
            Some(AlertCandidate {
                key: format!("{}:{}", kind.as_str(), connection.id),
                kind,
                severity: AlertSeverity::Warning,
                title,
                message,
            })
        })
        .collect()
}

/// Une alerte (unique) tant qu'au moins un cours automatique n'est plus mis à jour.
pub fn stale_prices_alert(names: &[String]) -> Option<AlertCandidate> {
    if names.is_empty() {
        return None;
    }
    let title = match names.len() {
        1 => "1 cours n'est plus mis à jour".to_string(),
        count => format!("{count} cours ne sont plus mis à jour"),
    };
    Some(AlertCandidate {
        key: "stale_prices".to_string(),
        kind: AlertKind::StalePrices,
        severity: AlertSeverity::Info,
        title,
        message: format!("Cours à vérifier : {}.", names.join(", ")),
    })
}

/// Toutes les conditions détectées par les règles activées.
pub fn evaluate(rules: &AlertRules, inputs: &AlertInputs) -> Vec<AlertCandidate> {
    let mut candidates = Vec::new();
    if rules.active(AlertRuleKind::Goals).is_some() {
        candidates.extend(goal_alerts(&inputs.goals));
    }
    if let Some(rule) = rules.active(AlertRuleKind::SavingsCeiling) {
        candidates.extend(savings_ceiling_alerts(
            &inputs.savings,
            rule.threshold_pct.unwrap_or(DEFAULT_CEILING_THRESHOLD_PCT),
            rule.ceiling.unwrap_or(DEFAULT_SAVINGS_CEILING),
        ));
    }
    if let Some(rule) = rules.active(AlertRuleKind::NetWorthChange) {
        candidates.extend(net_worth_change_alert(
            inputs.net_worth,
            rule.threshold_pct.unwrap_or(DEFAULT_NET_WORTH_CHANGE_PCT),
        ));
    }
    if rules.active(AlertRuleKind::Connections).is_some() {
        candidates.extend(connection_alerts(&inputs.connections));
    }
    if rules.active(AlertRuleKind::StalePrices).is_some() {
        candidates.extend(stale_prices_alert(&inputs.stale_prices));
    }
    candidates
}

// ---------------------------------------------------------------- alertes enregistrées

#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    pub id: AlertId,
    pub key: String,
    pub kind: AlertKind,
    pub severity: AlertSeverity,
    pub title: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub read_at: Option<DateTime<Utc>>,
    pub archived_at: Option<DateTime<Utc>>,
    /// La condition a disparu à cette date.
    pub resolved_at: Option<DateTime<Utc>>,
}

impl Alert {
    fn open(candidate: AlertCandidate, id: AlertId, now: DateTime<Utc>) -> Self {
        Self {
            id,
            key: candidate.key,
            kind: candidate.kind,
            severity: candidate.severity,
            title: candidate.title,
            message: candidate.message,
            created_at: now,
            read_at: None,
            archived_at: None,
            resolved_at: None,
        }
    }

    pub fn is_active(&self) -> bool {
        self.resolved_at.is_none()
    }

    /// Marque lue (date conservée si déjà lue) ou non lue.
    pub fn set_read(&mut self, read: bool, now: DateTime<Utc>) {
        self.read_at = if read {
            self.read_at.or(Some(now))
        } else {
            None
        };
    }

    pub fn set_archived(&mut self, archived: bool, now: DateTime<Utc>) {
        self.archived_at = if archived {
            self.archived_at.or(Some(now))
        } else {
            None
        };
    }
}

/// Alertes à enregistrer après rapprochement, et compteurs.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Reconciliation {
    pub changed: Vec<Alert>,
    pub created: usize,
    pub reopened: usize,
    pub resolved: usize,
}

/// Rapproche les conditions détectées des alertes enregistrées (clés uniques) :
/// - condition nouvelle → alerte créée, non lue ;
/// - condition déjà signalée et toujours active → même alerte (état lu/archivé conservé), texte mis à jour ;
/// - condition signalée puis résolue → alerte rouverte (non lue, désarchivée) ;
/// - alerte active dont la condition a disparu → résolue.
pub fn reconcile(
    existing: &[Alert],
    candidates: Vec<AlertCandidate>,
    now: DateTime<Utc>,
    mut new_id: impl FnMut() -> AlertId,
) -> Reconciliation {
    let mut result = Reconciliation::default();
    let mut seen: Vec<String> = Vec::new();
    for candidate in candidates {
        if seen.contains(&candidate.key) {
            continue;
        }
        seen.push(candidate.key.clone());
        match existing.iter().find(|alert| alert.key == candidate.key) {
            None => {
                result.created += 1;
                result.changed.push(Alert::open(candidate, new_id(), now));
            }
            Some(alert) if !alert.is_active() => {
                result.reopened += 1;
                result.changed.push(Alert::open(candidate, alert.id, now));
            }
            Some(alert) if alert.title != candidate.title || alert.message != candidate.message => {
                result.changed.push(Alert {
                    title: candidate.title,
                    message: candidate.message,
                    severity: candidate.severity,
                    ..alert.clone()
                });
            }
            Some(_) => {}
        }
    }
    for alert in existing {
        if alert.is_active() && !seen.contains(&alert.key) {
            result.resolved += 1;
            result.changed.push(Alert {
                resolved_at: Some(now),
                ..alert.clone()
            });
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::ConnectionId;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn at(day: u32) -> DateTime<Utc> {
        d(2026, 3, day).and_hms_opt(7, 0, 0).unwrap().and_utc()
    }

    fn goal(status: GoalStatus) -> GoalState {
        GoalState {
            id: GoalId::generate(),
            name: "Voyage".into(),
            status,
        }
    }

    fn livret(value: f64) -> SavingsBalance {
        SavingsBalance {
            account_id: AccountId::generate(),
            name: "Livret A".into(),
            value,
        }
    }

    fn point(day: u32, net_worth: f64) -> NetWorthPoint {
        NetWorthPoint {
            date: d(2026, 3, day),
            net_worth,
        }
    }

    fn connection(status: ConnectionStatus, error: Option<&str>) -> Connection {
        Connection {
            id: ConnectionId::generate(),
            provider: "demo".into(),
            institution: Some("Banque Fictive".into()),
            status,
            last_sync_at: None,
            error: error.map(str::to_string),
            external_id: None,
        }
    }

    // ------------------------------------------------ règles

    #[test]
    fn rule_thresholds_are_validated_per_kind() {
        let ceiling = AlertRuleKind::SavingsCeiling;
        assert!(AlertRule::new(ceiling, true, Some(100.0), Some(10_000.0)).is_ok());
        for bad in [0.0, -5.0, 100.5, f64::NAN] {
            assert!(
                AlertRule::new(ceiling, true, Some(bad), Some(10_000.0)).is_err(),
                "{bad}"
            );
        }
        assert!(AlertRule::new(ceiling, true, Some(90.0), Some(0.0)).is_err());
        assert!(AlertRule::new(ceiling, true, None, Some(1.0)).is_err());
        let change = AlertRuleKind::NetWorthChange;
        assert!(AlertRule::new(change, true, Some(1000.0), None).is_ok());
        assert!(AlertRule::new(change, true, Some(1000.1), None).is_err());
        assert!(AlertRule::new(change, true, Some(5.0), Some(1.0)).is_err());
        assert!(AlertRule::new(AlertRuleKind::Goals, false, None, None).is_ok());
        assert!(AlertRule::new(AlertRuleKind::Goals, true, Some(5.0), None).is_err());
    }

    #[test]
    fn partial_update_keeps_other_fields_and_revalidates() {
        let rule = AlertRule::default_for(AlertRuleKind::SavingsCeiling);
        let disabled = rule.updated(Some(false), None, None).unwrap();
        assert!(!disabled.enabled());
        assert_eq!(
            disabled.threshold_pct(),
            Some(DEFAULT_CEILING_THRESHOLD_PCT)
        );
        assert_eq!(disabled.ceiling(), Some(DEFAULT_SAVINGS_CEILING));
        assert!(rule.updated(None, Some(150.0), None).is_err());
        let goals = AlertRule::default_for(AlertRuleKind::Goals);
        assert!(goals.updated(None, Some(10.0), None).is_err());
    }

    #[test]
    fn missing_rules_take_their_default() {
        let stored = vec![AlertRule::new(AlertRuleKind::Goals, false, None, None).unwrap()];
        let rules = AlertRules::with_defaults(stored);
        assert_eq!(rules.all().len(), AlertRuleKind::ALL.len());
        assert!(!rules.get(AlertRuleKind::Goals).enabled());
        assert!(rules.get(AlertRuleKind::StalePrices).enabled());
    }

    // ------------------------------------------------ objectifs

    #[test]
    fn reached_and_overdue_goals_raise_alerts_others_do_not() {
        let goals = [
            goal(GoalStatus::Completed),
            goal(GoalStatus::Funded),
            goal(GoalStatus::Overdue),
            goal(GoalStatus::OnTrack),
            goal(GoalStatus::Challenging),
            goal(GoalStatus::Underfunded),
        ];
        let alerts = goal_alerts(&goals);
        let kinds: Vec<AlertKind> = alerts.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            [
                AlertKind::GoalReached,
                AlertKind::GoalReached,
                AlertKind::GoalOverdue
            ]
        );
        assert_eq!(alerts[2].key, format!("goal_overdue:{}", goals[2].id));
        assert_eq!(alerts[2].severity, AlertSeverity::Warning);
    }

    // ------------------------------------------------ plafond des livrets

    #[test]
    fn savings_alert_starts_exactly_at_the_threshold() {
        let alerts = savings_ceiling_alerts(
            &[livret(8_999.99), livret(9_000.0), livret(12_000.0)],
            90.0,
            10_000.0,
        );
        assert_eq!(alerts.len(), 2);
        assert!(alerts[0].message.contains("90 %"), "{}", alerts[0].message);
        assert!(alerts[0].message.contains("10\u{202f}000 €"));
        // Au-delà du plafond, toujours signalé.
        assert!(alerts[1].message.contains("120 %"));
    }

    #[test]
    fn savings_rule_with_full_threshold_only_flags_a_reached_ceiling() {
        assert!(savings_ceiling_alerts(&[livret(9_999.0)], 100.0, 10_000.0).is_empty());
        assert_eq!(
            savings_ceiling_alerts(&[livret(10_000.0)], 100.0, 10_000.0).len(),
            1
        );
        assert!(savings_ceiling_alerts(&[livret(1.0)], 90.0, 0.0).is_empty());
    }

    // ------------------------------------------------ variation du patrimoine

    #[test]
    fn strong_change_in_either_direction_is_flagged() {
        let up =
            net_worth_change_alert(Some((point(1, 100_000.0), point(2, 110_000.0))), 10.0).unwrap();
        assert_eq!(up.key, "net_worth_change:2026-03-02");
        assert!(up.title.contains("hausse") && up.title.contains("+10.0 %"));
        let down =
            net_worth_change_alert(Some((point(1, 100_000.0), point(2, 85_000.0))), 10.0).unwrap();
        assert!(down.title.contains("baisse"), "{}", down.title);
        assert!(
            down.message.contains("-15\u{202f}000 €"),
            "{}",
            down.message
        );
    }

    #[test]
    fn small_change_missing_history_or_zero_base_raise_nothing() {
        assert!(
            net_worth_change_alert(Some((point(1, 100_000.0), point(2, 109_999.0))), 10.0)
                .is_none()
        );
        assert!(net_worth_change_alert(None, 10.0).is_none());
        assert!(net_worth_change_alert(Some((point(1, 0.0), point(2, 5_000.0))), 10.0).is_none());
    }

    #[test]
    fn change_is_relative_to_the_absolute_previous_value() {
        // Patrimoine négatif qui s'améliore : c'est une hausse.
        let alert =
            net_worth_change_alert(Some((point(1, -10_000.0), point(2, -8_000.0))), 10.0).unwrap();
        assert!(alert.title.contains("hausse") && alert.title.contains("+20.0 %"));
    }

    // ------------------------------------------------ connexions et cours

    #[test]
    fn expired_and_failing_connections_are_flagged_separately() {
        let alerts = connection_alerts(&[
            connection(ConnectionStatus::Active, None),
            connection(ConnectionStatus::Expired, None),
            connection(ConnectionStatus::Error, Some("Identifiants refusés")),
            connection(ConnectionStatus::Error, None),
        ]);
        let kinds: Vec<AlertKind> = alerts.iter().map(|a| a.kind).collect();
        assert_eq!(
            kinds,
            [
                AlertKind::ConnectionExpired,
                AlertKind::ConnectionError,
                AlertKind::ConnectionError
            ]
        );
        assert!(alerts[0].title.contains("Banque Fictive"));
        assert_eq!(alerts[1].message, "Identifiants refusés");
        assert!(!alerts[2].message.is_empty());
    }

    #[test]
    fn stale_prices_are_grouped_in_one_alert() {
        assert!(stale_prices_alert(&[]).is_none());
        let one = stale_prices_alert(&["ETF Démo".into()]).unwrap();
        assert_eq!(one.title, "1 cours n'est plus mis à jour");
        let two = stale_prices_alert(&["ETF Démo".into(), "Action Fictive".into()]).unwrap();
        assert_eq!(two.key, one.key);
        assert!(two.message.contains("ETF Démo, Action Fictive"));
    }

    #[test]
    fn disabled_rules_are_not_evaluated() {
        let inputs = AlertInputs {
            goals: vec![goal(GoalStatus::Overdue)],
            savings: vec![livret(30_000.0)],
            net_worth: Some((point(1, 100.0), point(2, 1_000.0))),
            connections: vec![connection(ConnectionStatus::Expired, None)],
            stale_prices: vec!["ETF Démo".into()],
        };
        let all = AlertRules::with_defaults(Vec::new());
        assert_eq!(evaluate(&all, &inputs).len(), 5);
        let none = AlertRules::with_defaults(
            AlertRuleKind::ALL
                .iter()
                .map(|kind| AlertRule::default_for(*kind).updated(Some(false), None, None))
                .collect::<Result<_, _>>()
                .unwrap(),
        );
        assert!(evaluate(&none, &inputs).is_empty());
    }

    #[test]
    fn configured_ceiling_is_used() {
        let rule = AlertRule::default_for(AlertRuleKind::SavingsCeiling)
            .updated(None, Some(50.0), Some(1_000.0))
            .unwrap();
        let rules = AlertRules::with_defaults(vec![rule]);
        let inputs = AlertInputs {
            savings: vec![livret(500.0)],
            ..AlertInputs::default()
        };
        assert_eq!(evaluate(&rules, &inputs).len(), 1);
    }

    // ------------------------------------------------ rapprochement

    fn candidate(key: &str, message: &str) -> AlertCandidate {
        AlertCandidate {
            key: key.into(),
            kind: AlertKind::StalePrices,
            severity: AlertSeverity::Info,
            title: "Titre".into(),
            message: message.into(),
        }
    }

    #[test]
    fn same_condition_does_not_create_a_second_alert() {
        let first = reconcile(&[], vec![candidate("k", "m")], at(1), AlertId::generate);
        assert_eq!(first.created, 1);
        let mut stored = first.changed;
        stored[0].set_read(true, at(1));
        stored[0].set_archived(true, at(1));

        // Le lendemain, même condition, même texte : rien à enregistrer, l'état est conservé.
        let second = reconcile(&stored, vec![candidate("k", "m")], at(2), AlertId::generate);
        assert_eq!(second, Reconciliation::default());

        // Texte différent : la même alerte est mise à jour sans redevenir non lue.
        let third = reconcile(
            &stored,
            vec![candidate("k", "m2")],
            at(3),
            AlertId::generate,
        );
        assert_eq!((third.created, third.reopened), (0, 0));
        assert_eq!(third.changed.len(), 1);
        assert_eq!(third.changed[0].id, stored[0].id);
        assert_eq!(third.changed[0].message, "m2");
        assert!(third.changed[0].read_at.is_some() && third.changed[0].archived_at.is_some());
    }

    #[test]
    fn vanished_condition_resolves_and_its_return_reopens() {
        let stored = reconcile(&[], vec![candidate("k", "m")], at(1), AlertId::generate).changed;
        let resolved = reconcile(&stored, Vec::new(), at(2), AlertId::generate);
        assert_eq!(resolved.resolved, 1);
        assert_eq!(resolved.changed[0].resolved_at, Some(at(2)));

        // Une alerte déjà résolue ne l'est pas deux fois.
        let again = reconcile(&resolved.changed, Vec::new(), at(3), AlertId::generate);
        assert_eq!(again, Reconciliation::default());

        let mut archived = resolved.changed.clone();
        archived[0].set_read(true, at(2));
        archived[0].set_archived(true, at(2));
        let reopened = reconcile(
            &archived,
            vec![candidate("k", "m")],
            at(4),
            AlertId::generate,
        );
        assert_eq!(reopened.reopened, 1);
        let alert = &reopened.changed[0];
        assert_eq!(alert.id, stored[0].id);
        assert_eq!(alert.created_at, at(4));
        assert!(alert.read_at.is_none() && alert.archived_at.is_none() && alert.is_active());
    }

    #[test]
    fn duplicate_candidates_count_once() {
        let result = reconcile(
            &[],
            vec![candidate("k", "m"), candidate("k", "m")],
            at(1),
            AlertId::generate,
        );
        assert_eq!(result.created, 1);
        assert_eq!(result.changed.len(), 1);
    }

    #[test]
    fn read_and_archive_flags_keep_their_first_date() {
        let mut alert = reconcile(&[], vec![candidate("k", "m")], at(1), AlertId::generate)
            .changed
            .remove(0);
        alert.set_read(true, at(2));
        alert.set_read(true, at(3));
        assert_eq!(alert.read_at, Some(at(2)));
        alert.set_read(false, at(4));
        assert!(alert.read_at.is_none());
        alert.set_archived(true, at(5));
        alert.set_archived(false, at(6));
        assert!(alert.archived_at.is_none());
    }

    #[test]
    fn amounts_are_grouped_by_thousands() {
        assert_eq!(eur(0.4), "0 €");
        assert_eq!(eur(999.6), "1\u{202f}000 €");
        assert_eq!(eur(1_234_567.0), "1\u{202f}234\u{202f}567 €");
        assert_eq!(eur(-22_950.0), "-22\u{202f}950 €");
    }
}
