//! Résultats des tools : JSON compact, sous-ensembles des réponses REST du contrat (`docs/API.md`). La forme de
//! chaque type vient de `fineme_http::output`, pour n'avoir qu'une sérialisation par type ; ce module ne fait que
//! choisir les champs utiles à un client IA.

use fineme_application::budget::BudgetMonth;
use fineme_application::goals::GoalView;
use fineme_application::overview::{HistoryComparison, OverviewSummary};
use fineme_application::performance::PerformanceView;
use fineme_domain::alert::Alert;
use fineme_http::contract as rest;
use serde_json::{Map, Value, json};

/// Garde seulement `keys` d'un objet JSON, dans cet ordre.
fn pick(value: &Value, keys: &[&str]) -> Value {
    let mut picked = Map::new();
    for key in keys {
        if let Some(field) = value.get(*key) {
            picked.insert((*key).to_string(), field.clone());
        }
    }
    Value::Object(picked)
}

/// Retire un champ d'un objet JSON (sans effet si ce n'est pas un objet).
fn without(mut value: Value, key: &str) -> Value {
    if let Value::Object(map) = &mut value {
        map.remove(key);
    }
    value
}

pub fn overview(summary: &OverviewSummary) -> Value {
    let full = rest::overview(&summary.overview);
    let members: Vec<Value> = full["members"]
        .as_array()
        .map(|members| {
            members
                .iter()
                .map(|m| pick(m, &["member_id", "name", "net_worth"]))
                .collect()
        })
        .unwrap_or_default();
    let allocation: Vec<Value> = full["allocation"]
        .as_array()
        .map(|slices| {
            slices
                .iter()
                .map(|s| pick(s, &["category", "value", "pct"]))
                .collect()
        })
        .unwrap_or_default();
    json!({
        "as_of": summary.as_of,
        "view": {
            "member_id": summary.view.member().map(|id| id.as_uuid()),
            "name": summary.member_name.as_deref().unwrap_or("Foyer"),
        },
        "net_worth": full["net_worth"],
        "gross_assets": full["gross_assets"],
        "liabilities": full["liabilities"],
        "financial_assets": full["financial_assets"],
        "invested": full["invested"],
        "gain": full["gain"],
        "change": summary.change.map(|change| json!({
            "period": summary.period.as_str(),
            "from_date": change.from_date,
            "net_worth_start": change.net_worth_start,
            "delta": change.delta,
            "delta_pct": change.delta_pct,
        })),
        "allocation": allocation,
        "members": members,
        "top_holdings": full["top_holdings"],
        "weighted_expense_ratio": full["weighted_expense_ratio"],
    })
}

pub fn performance(view: &PerformanceView) -> Value {
    let full = rest::performance(view);
    let accounts: Vec<Value> = full["accounts"]
        .as_array()
        .map(|accounts| {
            accounts
                .iter()
                .map(|a| {
                    let mut account = pick(a, &["account_id", "name", "kind"]);
                    account["performance"] = without(a["performance"].clone(), "points");
                    account
                })
                .collect()
        })
        .unwrap_or_default();
    let benchmark = match &full["benchmark"] {
        Value::Null => Value::Null,
        b => pick(b, &["name", "symbol", "comparison", "message"]),
    };
    json!({
        "portfolio": without(full["portfolio"].clone(), "points"),
        "accounts": accounts,
        "benchmark": benchmark,
    })
}

pub fn budget(month: &BudgetMonth) -> Value {
    without(rest::budget_month(month), "trend")
}

pub fn goals(goals: &[GoalView]) -> Value {
    goals
        .iter()
        .map(|goal| {
            pick(
                &rest::goal(goal),
                &[
                    "id",
                    "name",
                    "kind",
                    "target_date",
                    "monthly_contribution",
                    "progress",
                ],
            )
        })
        .collect()
}

pub fn comparison(result: &HistoryComparison) -> Value {
    rest::history_comparison(result)
}

pub fn alerts(alerts: &[Alert]) -> Value {
    alerts
        .iter()
        .map(|alert| {
            pick(
                &rest::alert(alert),
                &[
                    "kind",
                    "severity",
                    "title",
                    "message",
                    "created_at",
                    "read",
                    "active",
                ],
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_keeps_listed_fields_and_nulls() {
        let value = json!({ "a": 1, "b": null, "c": 3 });
        assert_eq!(pick(&value, &["b", "a", "z"]), json!({ "b": null, "a": 1 }));
        assert_eq!(without(value, "c"), json!({ "a": 1, "b": null }));
        assert_eq!(without(Value::Null, "c"), Value::Null);
    }
}
