//! Budget : catégorisation des transactions, virements internes, flux mensuels et charges récurrentes.
//! Fonctions pures ; les montants manipulés ici sont déjà en EUR et multipliés par la part de la vue.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;

use chrono::{Datelike, Months, NaiveDate};

use crate::error::{DomainError, non_negative};
use crate::ids::{AccountId, BudgetRuleId, TransactionId};
use crate::property::string_enum;
use crate::transaction::Transaction;

// ---------------------------------------------------------------- mois

/// Mois calendaire (`YYYY-MM`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Month(NaiveDate);

impl Month {
    pub fn of(date: NaiveDate) -> Self {
        Self(date.with_day(1).unwrap_or(date))
    }

    /// Lit `YYYY-MM`.
    pub fn parse(value: &str) -> Result<Self, DomainError> {
        NaiveDate::parse_from_str(&format!("{}-01", value.trim()), "%Y-%m-%d")
            .map(Self)
            .map_err(|_| DomainError::Rule("Mois invalide (format AAAA-MM attendu)"))
    }

    pub fn first_day(&self) -> NaiveDate {
        self.0
    }

    pub fn last_day(&self) -> NaiveDate {
        self.next().0.pred_opt().unwrap_or(self.0)
    }

    pub fn contains(&self, date: NaiveDate) -> bool {
        Month::of(date) == *self
    }

    pub fn next(&self) -> Self {
        Self(self.0.checked_add_months(Months::new(1)).unwrap_or(self.0))
    }

    /// Le mois situé `count` mois plus tôt.
    pub fn back(&self, count: u32) -> Self {
        Self(
            self.0
                .checked_sub_months(Months::new(count))
                .unwrap_or(self.0),
        )
    }
}

impl fmt::Display for Month {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format("%Y-%m"))
    }
}

// ---------------------------------------------------------------- libellés et catégories

/// Libellé normalisé : minuscules, sans accents, chiffres et ponctuation remplacés par des espaces, espaces réduits.
pub fn normalize_label(label: &str) -> String {
    let mut text = String::with_capacity(label.len());
    for c in label.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => text.push('a'),
            'é' | 'è' | 'ê' | 'ë' => text.push('e'),
            'î' | 'ï' | 'í' => text.push('i'),
            'ô' | 'ö' | 'ó' | 'õ' => text.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => text.push('u'),
            'ÿ' => text.push('y'),
            'ç' => text.push('c'),
            'ñ' => text.push('n'),
            'œ' => text.push_str("oe"),
            'æ' => text.push_str("ae"),
            c if c.is_ascii_lowercase() => text.push(c),
            _ => text.push(' '),
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Vrai si `pattern` (normalisé) apparaît dans `label` (normalisé) sur des limites de mots.
fn matches_words(label: &str, pattern: &str) -> bool {
    !pattern.is_empty() && format!(" {label} ").contains(&format!(" {pattern} "))
}

string_enum!(
    /// Origine de la catégorie effective d'une transaction.
    CategorySource { Manual => "manual", Rule => "rule", Import => "import", Default => "default" }
    invalid = "Origine de catégorie invalide"
);

/// Règle utilisateur : un libellé normalisé contenant `pattern` reçoit `category`.
#[derive(Debug, Clone, PartialEq)]
pub struct CategoryRule {
    pub id: BudgetRuleId,
    pattern: String,
    category: String,
}

impl CategoryRule {
    pub fn new(id: BudgetRuleId, pattern: &str, category: &str) -> Result<Self, DomainError> {
        let pattern = normalize_label(pattern);
        if pattern.is_empty() {
            return Err(DomainError::Rule(
                "Le motif d'une règle doit contenir au moins une lettre",
            ));
        }
        Ok(Self {
            id,
            pattern,
            category: category_name(category)?,
        })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn category(&self) -> &str {
        &self.category
    }
}

/// Nom de catégorie nettoyé et non vide.
pub fn category_name(raw: &str) -> Result<String, DomainError> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        Err(DomainError::Rule("Le nom de la catégorie est requis"))
    } else if name.chars().count() > 60 {
        Err(DomainError::Rule(
            "Le nom de la catégorie ne peut pas dépasser 60 caractères",
        ))
    } else {
        Ok(name)
    }
}

/// Mots-clés génériques (libellés normalisés) appliqués en dernier recours.
const DEFAULT_RULES: &[(&str, &str)] = &[
    ("salaire", "Revenus"),
    ("paie", "Revenus"),
    ("remuneration", "Revenus"),
    ("pension", "Revenus"),
    ("caf", "Revenus"),
    ("france travail", "Revenus"),
    ("loyer", "Logement"),
    ("edf", "Logement"),
    ("engie", "Logement"),
    ("veolia", "Logement"),
    ("syndic", "Logement"),
    ("carrefour", "Alimentation"),
    ("leclerc", "Alimentation"),
    ("auchan", "Alimentation"),
    ("lidl", "Alimentation"),
    ("aldi", "Alimentation"),
    ("intermarche", "Alimentation"),
    ("monoprix", "Alimentation"),
    ("franprix", "Alimentation"),
    ("picard", "Alimentation"),
    ("super u", "Alimentation"),
    ("biocoop", "Alimentation"),
    ("sncf", "Transport"),
    ("ratp", "Transport"),
    ("navigo", "Transport"),
    ("uber", "Transport"),
    ("blablacar", "Transport"),
    ("carburant", "Transport"),
    ("peage", "Transport"),
    ("esso", "Transport"),
    ("uber eats", "Restaurants"),
    ("deliveroo", "Restaurants"),
    ("restaurant", "Restaurants"),
    ("mcdonald s", "Restaurants"),
    ("burger king", "Restaurants"),
    ("netflix", "Abonnements"),
    ("spotify", "Abonnements"),
    ("deezer", "Abonnements"),
    ("disney plus", "Abonnements"),
    ("amazon prime", "Abonnements"),
    ("free mobile", "Abonnements"),
    ("orange", "Abonnements"),
    ("sfr", "Abonnements"),
    ("bouygues", "Abonnements"),
    ("sosh", "Abonnements"),
    ("pharmacie", "Santé"),
    ("mutuelle", "Santé"),
    ("medecin", "Santé"),
    ("dentiste", "Santé"),
    ("doctolib", "Santé"),
    ("dgfip", "Impôts"),
    ("impot", "Impôts"),
    ("impots", "Impôts"),
    ("finances publiques", "Impôts"),
    ("amazon", "Achats"),
    ("fnac", "Achats"),
    ("ikea", "Achats"),
    ("decathlon", "Achats"),
    ("cinema", "Loisirs"),
    ("retrait", "Retraits"),
    ("frais", "Banque"),
    ("cotisation", "Banque"),
    ("commission", "Banque"),
];

/// Catégories proposées par défaut, sans doublon.
pub fn default_categories() -> BTreeSet<String> {
    DEFAULT_RULES
        .iter()
        .map(|(_, category)| (*category).to_string())
        .collect()
}

/// Règles utilisateur prêtes à appliquer : motif le plus long d'abord.
#[derive(Debug, Clone, Default)]
pub struct Categorizer {
    rules: Vec<CategoryRule>,
}

impl Categorizer {
    pub fn new(mut rules: Vec<CategoryRule>) -> Self {
        rules.sort_by(|a, b| {
            b.pattern
                .len()
                .cmp(&a.pattern.len())
                .then_with(|| a.pattern.cmp(&b.pattern))
        });
        Self { rules }
    }

    /// Catégorie effective : manuelle, puis règle utilisateur, puis import, puis règle par défaut.
    pub fn categorize(&self, transaction: &Transaction) -> Option<(String, CategorySource)> {
        if let Some(manual) = &transaction.manual_category {
            return Some((manual.clone(), CategorySource::Manual));
        }
        let label = normalize_label(&transaction.label);
        if let Some(rule) = self
            .rules
            .iter()
            .find(|rule| matches_words(&label, &rule.pattern))
        {
            return Some((rule.category.clone(), CategorySource::Rule));
        }
        if let Some(imported) = &transaction.category {
            return Some((imported.clone(), CategorySource::Import));
        }
        default_category(&label).map(|category| (category.to_string(), CategorySource::Default))
    }
}

fn default_category(label: &str) -> Option<&'static str> {
    DEFAULT_RULES
        .iter()
        .filter(|(pattern, _)| matches_words(label, pattern))
        .max_by_key(|(pattern, _)| pattern.len())
        .map(|(_, category)| *category)
}

// ---------------------------------------------------------------- virements internes

/// Écart maximal, en jours, entre les deux jambes d'un virement interne.
pub const TRANSFER_MAX_DAYS: i64 = 3;

/// Virements internes détectés automatiquement : paires de transactions de comptes différents, de montants
/// opposés (écart < 0,01) et à `TRANSFER_MAX_DAYS` jours ou moins. Chaque transaction est appariée au plus une
/// fois, l'écart de dates le plus court d'abord. Les transactions marquées à la main ne sont pas appariées.
pub fn detect_internal_transfers(transactions: &[Transaction]) -> HashSet<TransactionId> {
    let mut by_cents: HashMap<i64, Vec<&Transaction>> = HashMap::new();
    for transaction in transactions {
        if transaction.transfer_override.is_none() && transaction.amount != 0.0 {
            by_cents
                .entry(cents(transaction.amount.abs()))
                .or_default()
                .push(transaction);
        }
    }
    let mut pairs: Vec<(i64, NaiveDate, TransactionId, TransactionId)> = Vec::new();
    for group in by_cents.values() {
        for (index, a) in group.iter().enumerate() {
            for b in &group[index + 1..] {
                let gap = (a.date - b.date).num_days().abs();
                if a.account_id != b.account_id
                    && (a.amount + b.amount).abs() < 0.01
                    && gap <= TRANSFER_MAX_DAYS
                {
                    let (first, second) = if a.id < b.id { (a, b) } else { (b, a) };
                    pairs.push((gap, first.date.min(second.date), first.id, second.id));
                }
            }
        }
    }
    pairs.sort();
    let mut matched = HashSet::new();
    for (_, _, a, b) in pairs {
        if !matched.contains(&a) && !matched.contains(&b) {
            matched.insert(a);
            matched.insert(b);
        }
    }
    matched
}

fn cents(amount: f64) -> i64 {
    // Montants bornés (finis, contrôlés à l'import) : la conversion ne déborde pas en pratique.
    (amount * 100.0).round() as i64
}

/// Virement interne effectif : le marquage manuel l'emporte sur la détection.
pub fn is_internal(transaction: &Transaction, detected: &HashSet<TransactionId>) -> bool {
    transaction
        .transfer_override
        .unwrap_or_else(|| detected.contains(&transaction.id))
}

// ---------------------------------------------------------------- flux mensuels

/// Transaction prête pour le budget : montant signé en EUR × part de la vue.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetLine {
    pub account_id: AccountId,
    pub date: NaiveDate,
    pub label: String,
    pub amount: f64,
    pub category: Option<String>,
    pub internal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flows {
    pub month: Month,
    pub income: f64,
    pub expenses: f64,
    pub savings: f64,
    pub savings_rate: Option<f64>,
}

/// Taux d'épargne = (revenus − dépenses) / revenus ; `None` sans revenu.
pub fn savings_rate(income: f64, expenses: f64) -> Option<f64> {
    (income > 0.0).then(|| (income - expenses) / income)
}

fn counted(lines: &[BudgetLine], month: Month) -> impl Iterator<Item = &BudgetLine> {
    lines
        .iter()
        .filter(move |line| !line.internal && month.contains(line.date))
}

/// Revenus, dépenses, épargne et taux d'épargne d'un mois (hors virements internes).
pub fn flows(lines: &[BudgetLine], month: Month) -> Flows {
    let (mut income, mut expenses) = (0.0, 0.0);
    for line in counted(lines, month) {
        if line.amount > 0.0 {
            income += line.amount;
        } else {
            expenses -= line.amount;
        }
    }
    Flows {
        month,
        income,
        expenses,
        savings: income - expenses,
        savings_rate: savings_rate(income, expenses),
    }
}

/// Flux des `count` mois se terminant par `last` inclus, du plus ancien au plus récent.
pub fn trend(lines: &[BudgetLine], last: Month, count: u32) -> Vec<Flows> {
    (0..count)
        .rev()
        .map(|back| flows(lines, last.back(back)))
        .collect()
}

/// Dépenses du mois par catégorie (`None` = non catégorisé), montants positifs, du plus gros au plus petit.
pub fn expenses_by_category(lines: &[BudgetLine], month: Month) -> Vec<(Option<String>, f64)> {
    let mut totals: BTreeMap<Option<String>, f64> = BTreeMap::new();
    for line in counted(lines, month).filter(|line| line.amount < 0.0) {
        *totals.entry(line.category.clone()).or_default() -= line.amount;
    }
    let mut result: Vec<_> = totals.into_iter().collect();
    result.sort_by(|a, b| b.1.total_cmp(&a.1));
    result
}

// ---------------------------------------------------------------- charges récurrentes

/// Écart toléré autour du montant médian d'une charge récurrente.
const RECURRING_TOLERANCE: f64 = 0.15;
const RECURRING_MIN_MONTHS: usize = 3;
const MONTHLY_INTERVAL_DAYS: std::ops::RangeInclusive<f64> = 25.0..=35.0;

#[derive(Debug, Clone, PartialEq)]
pub struct RecurringCharge {
    /// Libellé de la dernière occurrence.
    pub label: String,
    pub category: Option<String>,
    /// Montant moyen, positif.
    pub average_amount: f64,
    pub occurrences: usize,
    pub last_date: NaiveDate,
    pub next_date: NaiveDate,
}

/// Charges mensuelles : même libellé normalisé, au moins 3 mois distincts, montants à ±15 % de la médiane,
/// intervalle moyen entre 25 et 35 jours. Triées par montant décroissant.
pub fn detect_recurring(lines: &[BudgetLine]) -> Vec<RecurringCharge> {
    let mut groups: BTreeMap<String, Vec<&BudgetLine>> = BTreeMap::new();
    for line in lines
        .iter()
        .filter(|line| !line.internal && line.amount < 0.0)
    {
        groups
            .entry(normalize_label(&line.label))
            .or_default()
            .push(line);
    }
    let mut charges: Vec<RecurringCharge> = groups.into_values().filter_map(recurring).collect();
    charges.sort_by(|a, b| b.average_amount.total_cmp(&a.average_amount));
    charges
}

fn recurring(mut occurrences: Vec<&BudgetLine>) -> Option<RecurringCharge> {
    let median = median(occurrences.iter().map(|line| -line.amount).collect())?;
    occurrences.retain(|line| (-line.amount - median).abs() <= median * RECURRING_TOLERANCE);
    occurrences.sort_by_key(|line| line.date);
    let months: BTreeSet<Month> = occurrences
        .iter()
        .map(|line| Month::of(line.date))
        .collect();
    if months.len() < RECURRING_MIN_MONTHS {
        return None;
    }
    let (first, last) = (occurrences.first()?, occurrences.last()?);
    let span = (last.date - first.date).num_days() as f64;
    let interval = span / (occurrences.len() - 1) as f64;
    if !MONTHLY_INTERVAL_DAYS.contains(&interval) {
        return None;
    }
    let total: f64 = occurrences.iter().map(|line| -line.amount).sum();
    Some(RecurringCharge {
        label: last.label.clone(),
        category: last.category.clone(),
        average_amount: total / occurrences.len() as f64,
        occurrences: occurrences.len(),
        last_date: last.date,
        next_date: last
            .date
            .checked_add_months(Months::new(1))
            .unwrap_or(last.date),
    })
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    })
}

// ---------------------------------------------------------------- cibles

/// Budget mensuel visé pour une catégorie (EUR, ≥ 0).
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetTarget {
    category: String,
    monthly_amount: f64,
}

impl BudgetTarget {
    pub fn new(category: &str, monthly_amount: f64) -> Result<Self, DomainError> {
        Ok(Self {
            category: category_name(category)?,
            monthly_amount: non_negative(monthly_amount, "monthly_amount")?,
        })
    }

    pub fn category(&self) -> &str {
        &self.category
    }

    pub fn monthly_amount(&self) -> f64 {
        self.monthly_amount
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).unwrap()
    }

    fn tx(account: AccountId, date: NaiveDate, label: &str, amount: f64) -> Transaction {
        Transaction {
            id: TransactionId::generate(),
            account_id: account,
            date,
            label: label.into(),
            amount,
            category: None,
            manual_category: None,
            transfer_override: None,
        }
    }

    fn line(date: NaiveDate, label: &str, amount: f64, category: Option<&str>) -> BudgetLine {
        BudgetLine {
            account_id: AccountId::from_uuid(uuid::Uuid::nil()),
            date,
            label: label.into(),
            amount,
            category: category.map(String::from),
            internal: false,
        }
    }

    #[test]
    fn month_parsing_and_navigation() {
        let march = Month::parse("2026-03").unwrap();
        assert_eq!(march.to_string(), "2026-03");
        assert_eq!(march.last_day(), d(3, 31));
        assert_eq!(march.back(3).to_string(), "2025-12");
        assert_eq!(Month::parse("2024-02").unwrap().last_day().day(), 29);
        assert!(march.contains(d(3, 15)) && !march.contains(d(4, 1)));
        assert!(Month::parse("2026-13").is_err());
        assert!(Month::parse("mars").is_err());
    }

    #[test]
    fn normalizes_labels() {
        assert_eq!(normalize_label("PRLV SEPA EDF 0423/11"), "prlv sepa edf");
        assert_eq!(
            normalize_label("  Prélèvement  Œuvre   Crèche "),
            "prelevement oeuvre creche"
        );
        assert_eq!(normalize_label("CB12CARREFOUR"), "cb carrefour");
        assert_eq!(normalize_label("1234 / 56"), "");
    }

    #[test]
    fn categorization_priority() {
        let account = AccountId::generate();
        let rule =
            CategoryRule::new(BudgetRuleId::generate(), "PRLV Netflix.com", "Loisirs").unwrap();
        assert_eq!(rule.pattern(), "prlv netflix com");
        let categorizer = Categorizer::new(vec![rule]);
        let mut t = tx(account, d(1, 5), "PRLV NETFLIX.COM 8812", -13.49);
        // Règle utilisateur avant la catégorie importée et la règle par défaut.
        t.category = Some("Divertissement".into());
        assert_eq!(
            categorizer.categorize(&t),
            Some(("Loisirs".into(), CategorySource::Rule))
        );
        t.manual_category = Some("Abonnements".into());
        assert_eq!(
            categorizer.categorize(&t),
            Some(("Abonnements".into(), CategorySource::Manual))
        );
        let mut imported = tx(account, d(1, 5), "CB CARREFOUR", -20.0);
        imported.category = Some("Courses".into());
        assert_eq!(
            categorizer.categorize(&imported),
            Some(("Courses".into(), CategorySource::Import))
        );
        imported.category = None;
        assert_eq!(
            categorizer.categorize(&imported),
            Some(("Alimentation".into(), CategorySource::Default))
        );
        assert_eq!(
            categorizer.categorize(&tx(account, d(1, 5), "XYZ", -1.0)),
            None
        );
    }

    #[test]
    fn rules_match_whole_words_and_longest_pattern_wins() {
        let account = AccountId::generate();
        let categorizer = Categorizer::new(vec![
            CategoryRule::new(BudgetRuleId::generate(), "uber", "Transport").unwrap(),
            CategoryRule::new(BudgetRuleId::generate(), "uber eats", "Restaurants").unwrap(),
        ]);
        let category = |label: &str| categorizer.categorize(&tx(account, d(1, 1), label, -1.0));
        assert_eq!(category("CB UBER EATS PARIS").unwrap().0, "Restaurants");
        assert_eq!(category("CB UBER TRIP").unwrap().0, "Transport");
        // « caf » ne doit pas reconnaître « cafe » (limites de mots).
        assert_eq!(category("CB CAFE DE LA GARE"), None);
        assert!(CategoryRule::new(BudgetRuleId::generate(), "12/03", "X").is_err());
        assert!(CategoryRule::new(BudgetRuleId::generate(), "abc", "  ").is_err());
    }

    #[test]
    fn detects_internal_transfers_between_household_accounts() {
        let (checking, savings) = (AccountId::generate(), AccountId::generate());
        let out = tx(checking, d(2, 1), "VIR VERS LIVRET", -500.0);
        let back = tx(savings, d(2, 3), "VIR DEPUIS COMPTE", 500.0);
        let same_account = tx(checking, d(2, 1), "REMBOURSEMENT", 500.0);
        let too_late = tx(savings, d(2, 10), "VIR", -200.0);
        let late_in = tx(checking, d(2, 5), "VIR", 200.0);
        let all = [
            out.clone(),
            back.clone(),
            same_account.clone(),
            too_late.clone(),
            late_in.clone(),
        ];
        let detected = detect_internal_transfers(&all);
        assert!(detected.contains(&out.id) && detected.contains(&back.id));
        assert!(!detected.contains(&same_account.id));
        assert!(!detected.contains(&too_late.id) && !detected.contains(&late_in.id));
        // Marquage manuel : l'emporte et sort la transaction de l'appariement.
        let mut forced = late_in.clone();
        forced.transfer_override = Some(true);
        assert!(is_internal(&forced, &detected));
        let mut refused = out.clone();
        refused.transfer_override = Some(false);
        let detected = detect_internal_transfers(&[refused.clone(), back.clone()]);
        assert!(!is_internal(&refused, &detected) && !is_internal(&back, &detected));
    }

    #[test]
    fn each_transaction_is_paired_once_closest_first() {
        let (a, b) = (AccountId::generate(), AccountId::generate());
        let out = tx(a, d(3, 10), "VIR", -100.0);
        let near = tx(b, d(3, 10), "VIR", 100.0);
        let far = tx(b, d(3, 12), "VIR", 100.0);
        let detected = detect_internal_transfers(&[far.clone(), out.clone(), near.clone()]);
        assert_eq!(detected.len(), 2);
        assert!(detected.contains(&near.id) && !detected.contains(&far.id));
    }

    #[test]
    fn monthly_flows_and_savings_rate() {
        let mut internal = line(d(1, 20), "VIR LIVRET", -800.0, None);
        internal.internal = true;
        let lines = vec![
            line(d(1, 1), "SALAIRE", 3000.0, Some("Revenus")),
            line(d(1, 3), "LOYER", -1000.0, Some("Logement")),
            line(d(1, 9), "CB CARREFOUR", -300.0, Some("Alimentation")),
            line(d(1, 15), "CB INCONNU", -200.0, None),
            internal,
            line(d(2, 1), "SALAIRE", 3000.0, Some("Revenus")),
        ];
        let january = flows(&lines, Month::parse("2026-01").unwrap());
        assert_eq!(
            (january.income, january.expenses, january.savings),
            (3000.0, 1500.0, 1500.0)
        );
        assert_eq!(january.savings_rate, Some(0.5));
        assert_eq!(
            expenses_by_category(&lines, january.month),
            vec![
                (Some("Logement".to_string()), 1000.0),
                (Some("Alimentation".to_string()), 300.0),
                (None, 200.0),
            ]
        );
        let trend = trend(&lines, Month::parse("2026-02").unwrap(), 3);
        assert_eq!(
            trend
                .iter()
                .map(|f| (f.month.to_string(), f.savings))
                .collect::<Vec<_>>(),
            vec![
                ("2025-12".into(), 0.0),
                ("2026-01".into(), 1500.0),
                ("2026-02".into(), 3000.0)
            ]
        );
        assert_eq!(trend[0].savings_rate, None);
    }

    #[test]
    fn savings_rate_edge_cases() {
        assert_eq!(savings_rate(0.0, 100.0), None);
        assert_eq!(savings_rate(1000.0, 1500.0), Some(-0.5));
        assert_eq!(savings_rate(1000.0, 0.0), Some(1.0));
    }

    #[test]
    fn detects_monthly_recurring_charges() {
        let lines = vec![
            line(d(1, 5), "PRLV EDF 001", -60.0, Some("Logement")),
            line(d(2, 5), "PRLV EDF 002", -64.0, Some("Logement")),
            line(d(3, 6), "PRLV EDF 003", -58.0, Some("Logement")),
            // Montant hors tolérance : ignoré, sans casser la détection.
            line(d(3, 20), "PRLV EDF 004", -250.0, Some("Logement")),
            // Seulement deux mois : pas encore récurrent.
            line(d(2, 1), "NETFLIX", -13.0, None),
            line(d(3, 1), "NETFLIX", -13.0, None),
            // Hebdomadaire : intervalle trop court.
            line(d(1, 1), "BOULANGERIE", -5.0, None),
            line(d(1, 8), "BOULANGERIE", -5.0, None),
            line(d(2, 1), "BOULANGERIE", -5.0, None),
            line(d(3, 1), "BOULANGERIE", -5.0, None),
            line(d(3, 8), "BOULANGERIE", -5.0, None),
        ];
        let charges = detect_recurring(&lines);
        assert_eq!(charges.len(), 1);
        let edf = &charges[0];
        assert_eq!(edf.label, "PRLV EDF 003");
        assert_eq!(edf.occurrences, 3);
        assert!((edf.average_amount - 182.0 / 3.0).abs() < 1e-9);
        assert_eq!((edf.last_date, edf.next_date), (d(3, 6), d(4, 6)));
    }

    #[test]
    fn budget_target_invariants() {
        assert_eq!(
            BudgetTarget::new(" Loisirs ", 150.0).unwrap().category(),
            "Loisirs"
        );
        assert!(BudgetTarget::new("Loisirs", -1.0).is_err());
        assert!(BudgetTarget::new("Loisirs", f64::NAN).is_err());
        assert!(BudgetTarget::new("", 10.0).is_err());
    }
}
