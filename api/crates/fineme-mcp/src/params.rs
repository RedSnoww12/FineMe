//! Paramètres des tools : schéma JSON décrit pour le client IA, lecture tolérante et messages en français
//! (les valeurs sont validées ici plutôt que par serde, pour que l'erreur dise quoi corriger).

use chrono::NaiveDate;
use fineme_domain::timeline::Period;
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use uuid::Uuid;

use crate::error::ToolError;

const MEMBER_ID: &str = "Identifiant (UUID) d'un membre du foyer, tel que renvoyé par get_overview \
     (champ members[].member_id). Absent : vue du foyer entier. Présent : seulement la part de ce membre.";

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct ViewPeriodParams {
    /// Période de comparaison, relative à aujourd'hui.
    #[schemars(
        description = "Période relative à aujourd'hui : 1m (1 mois), 3m, 6m, ytd (depuis le 1er janvier), \
                       1y (1 an) ou all (tout l'historique). Défaut : 1m.",
        extend("enum" = ["1m", "3m", "6m", "ytd", "1y", "all"])
    )]
    pub period: Option<String>,
    #[schemars(description = MEMBER_ID)]
    pub member_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct BudgetParams {
    #[schemars(
        description = "Mois au format AAAA-MM (ex. 2026-10). Défaut : le mois en cours.",
        regex(pattern = r"^\d{4}-\d{2}$")
    )]
    pub month: Option<String>,
    #[schemars(description = MEMBER_ID)]
    pub member_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct CompareParams {
    #[schemars(
        description = "Première date, au format AAAA-MM-JJ. L'instantané retenu est le dernier relevé ce jour-là \
                       ou avant.",
        regex(pattern = r"^\d{4}-\d{2}-\d{2}$")
    )]
    pub date_a: String,
    #[schemars(
        description = "Seconde date, au format AAAA-MM-JJ. Absente : même date que date_a (patrimoine à une date).",
        regex(pattern = r"^\d{4}-\d{2}-\d{2}$")
    )]
    pub date_b: Option<String>,
    #[schemars(description = MEMBER_ID)]
    pub member_id: Option<String>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(crate = "rmcp::schemars")]
pub struct AlertsParams {
    #[schemars(
        description = "true : alertes archivées seulement. Défaut false : alertes en cours."
    )]
    pub include_archived: Option<bool>,
}

/// Période demandée, `1m` par défaut.
pub fn period(value: Option<&str>) -> Result<Period, ToolError> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(Period::OneMonth),
        Some(value) => Period::parse(value).map_err(|_| {
            ToolError::invalid(format!(
                "Période « {value} » inconnue : utilisez 1m, 3m, 6m, ytd, 1y ou all."
            ))
        }),
    }
}

pub fn member_id(value: Option<&str>) -> Result<Option<Uuid>, ToolError> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(value) => Uuid::parse_str(value).map(Some).map_err(|_| {
            ToolError::invalid(format!(
                "member_id « {value} » n'est pas un identifiant valide : reprenez un member_id renvoyé par \
                 get_overview, ou omettez-le pour le foyer entier."
            ))
        }),
    }
}

pub fn date(field: &str, value: &str) -> Result<NaiveDate, ToolError> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").map_err(|_| {
        ToolError::invalid(format!(
            "{field} « {value} » n'est pas une date valide : format AAAA-MM-JJ attendu (ex. 2026-01-31)."
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::schemars;

    #[test]
    fn period_defaults_to_one_month_and_explains_unknown_values() {
        assert_eq!(period(None).unwrap(), Period::OneMonth);
        assert_eq!(period(Some(" ")).unwrap(), Period::OneMonth);
        assert_eq!(period(Some("ytd")).unwrap(), Period::YearToDate);
        let error = period(Some("2w")).unwrap_err().message();
        assert!(error.contains("2w") && error.contains("ytd"), "{error}");
    }

    #[test]
    fn member_id_is_optional_and_checked() {
        assert_eq!(member_id(None).unwrap(), None);
        let id = Uuid::new_v4();
        assert_eq!(member_id(Some(&id.to_string())).unwrap(), Some(id));
        assert!(
            member_id(Some("Alice"))
                .unwrap_err()
                .message()
                .contains("get_overview")
        );
    }

    #[test]
    fn dates_use_the_iso_format() {
        assert_eq!(
            date("date_a", "2026-01-31").unwrap(),
            NaiveDate::from_ymd_opt(2026, 1, 31).unwrap()
        );
        assert!(
            date("date_a", "31/01/2026")
                .unwrap_err()
                .message()
                .contains("AAAA-MM-JJ")
        );
    }

    #[test]
    fn schemas_describe_every_parameter() {
        let schema = serde_json::to_value(schemars::schema_for!(ViewPeriodParams)).unwrap();
        let period = &schema["properties"]["period"];
        assert!(period["description"].as_str().unwrap().contains("ytd"));
        assert_eq!(period["enum"][3], "ytd");
        assert!(schema["properties"]["member_id"]["description"].is_string());
        let compare = serde_json::to_value(schemars::schema_for!(CompareParams)).unwrap();
        assert_eq!(compare["required"], serde_json::json!(["date_a"]));
    }
}
