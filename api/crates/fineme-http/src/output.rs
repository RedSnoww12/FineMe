//! DTO de sortie : vues du domaine → JSON du contrat (`docs/API.md`), en `snake_case`.

use fineme_application::bundle::Bundle;
use fineme_application::commands::{
    AccountInput, GoalInput, HoldingInput, LoanInput, PropertyInput,
};
use fineme_application::daily::DailyReport;
use fineme_application::goals::GoalView;
use fineme_application::history::{HistoryReview, ReviewedPoint};
use fineme_application::ports::{PricePoint, SnapshotPoint};
use fineme_application::prices::RefreshReport;
use fineme_domain::account::AccountKind;
use fineme_domain::connection::Connection;
use fineme_domain::ids::AccountId;
use fineme_domain::loan::ScheduleRow;
use fineme_domain::login::LoginEvent;
use fineme_domain::member::{Member, User};
use fineme_domain::portfolio::{
    AccountDetail, AccountSnapshot, AccountView, HoldingView, LoanView, Overview, RealEstateReport,
    Totals,
};
use fineme_domain::property::Property;
use fineme_domain::transaction::Transaction;
use serde_json::{Map, Value, json};

pub fn user(user: &User) -> Value {
    json!({ "id": user.id.as_uuid(), "email": user.email, "name": user.name })
}

pub fn login_event(event: &LoginEvent) -> Value {
    json!({
        "at": event.at,
        "action": event.action.as_str(),
        "success": event.success,
        "ip": event.origin.ip,
        "user_agent": event.origin.user_agent,
    })
}

pub fn member(member: &Member) -> Value {
    json!({ "id": member.id.as_uuid(), "name": member.name, "color": member.color, "is_primary": member.is_primary })
}

pub fn account(view: &AccountView) -> Value {
    let a = &view.account;
    json!({
        "id": a.id.as_uuid(),
        "name": a.name,
        "institution": a.institution,
        "kind": a.kind.as_str(),
        "envelope": a.envelope,
        "currency": a.currency.as_str(),
        "cash_balance": a.cash_balance,
        "annual_yield_pct": a.annual_yield_pct,
        "source": a.source.as_str(),
        "external_id": a.external_id,
        "connection_id": a.connection_id.map(|id| id.as_uuid()),
        "opened_at": a.opened_at,
        "owners": view.owners.iter().map(|o| json!({
            "member_id": o.member_id.as_uuid(), "name": o.name, "share": o.share,
        })).collect::<Vec<_>>(),
        "value": view.valuation.value,
        "owned_value": view.owned_value,
        "is_liability": a.kind.is_liability(),
        "invested": view.valuation.invested,
        "gain": view.valuation.gain(),
        "updated_at": a.updated_at,
    })
}

pub fn holding(view: &HoldingView) -> Value {
    let h = &view.holding;
    json!({
        "id": h.id.as_uuid(),
        "account_id": h.account_id.as_uuid(),
        "kind": h.kind.as_str(),
        "name": h.name,
        "isin": h.isin,
        "symbol": h.symbol,
        "security_type": h.security_type,
        "quantity": h.quantity,
        "buy_price": h.buy_price,
        "current_price": h.current_price,
        "currency": h.currency.as_str(),
        "expense_ratio": h.expense_ratio,
        "price_source": h.price_source.as_str(),
        "price_updated_at": h.price_updated_at,
        "price_stale": view.price_stale,
        "value": view.valuation.value,
        "invested": view.valuation.invested,
        "gain": view.valuation.gain,
        "gain_pct": view.valuation.gain_pct,
    })
}

pub fn property(account_id: AccountId, p: &Property) -> Value {
    json!({
        "id": p.id.as_uuid(),
        "account_id": account_id.as_uuid(),
        "address": p.address,
        "category": p.category.as_str(),
        "building_type": p.building_type.as_str(),
        "surface_m2": p.surface_m2,
        "purchase_price": p.purchase_price,
        "purchase_fees": p.purchase_fees,
        "purchased_at": p.purchased_at,
        "current_value": p.current_value,
        "monthly_rent": p.monthly_rent,
        "monthly_charges": p.monthly_charges,
        "yearly_property_tax": p.yearly_property_tax,
    })
}

pub fn loan(view: &LoanView) -> Value {
    let terms = &view.loan.terms;
    json!({
        "id": view.loan.id.as_uuid(),
        "account_id": view.account_id.as_uuid(),
        "property_account_id": view.loan.property_account_id.map(|id| id.as_uuid()),
        "principal": terms.principal(),
        "interest_rate_pct": terms.interest_rate().value(),
        "insurance_rate_pct": terms.insurance_rate().value(),
        "duration_months": terms.duration_months(),
        "start_date": terms.start_date(),
        "monthly_payment": view.summary.monthly_payment,
        "monthly_insurance": view.summary.monthly_insurance,
        "remaining_capital": view.summary.remaining_capital,
        "end_date": view.summary.end_date,
        "total_interest": view.summary.total_interest,
        "total_insurance": view.summary.total_insurance,
    })
}

fn with(mut base: Value, extra: Value) -> Value {
    if let (Value::Object(target), Value::Object(fields)) = (&mut base, extra) {
        target.extend(fields);
    }
    base
}

pub fn account_detail(detail: &AccountDetail) -> Value {
    let account_id = detail.view.account.id;
    with(
        account(&detail.view),
        json!({
            "holdings": detail.holdings.iter().map(holding).collect::<Vec<_>>(),
            "property": detail.view.account.property.as_ref().map(|p| property(account_id, p)),
            "loan": detail.loan.as_ref().map(loan),
        }),
    )
}

pub fn schedule(view: &LoanView, rows: &[ScheduleRow]) -> Value {
    json!({
        "loan": loan(view),
        "rows": rows.iter().map(|row| json!({
            "month": row.month,
            "date": row.date,
            "payment": row.payment,
            "interest": row.interest,
            "principal": row.principal,
            "insurance": row.insurance,
            "remaining": row.remaining,
        })).collect::<Vec<_>>(),
    })
}

pub fn real_estate(report: &RealEstateReport) -> Value {
    json!({
        "properties": report.items.iter().map(|item| json!({
            "account": account(&item.account),
            "property": property(item.account.account.id, &item.property),
            "loans": item.loans.iter().map(|linked| with(loan(&linked.view), json!({ "name": linked.name }))).collect::<Vec<_>>(),
            "equity": item.equity,
            "gross_yield_pct": item.gross_yield_pct,
            "net_yield_pct": item.net_yield_pct,
            "latent_gain": item.latent_gain,
        })).collect::<Vec<_>>(),
        "totals": {
            "value": report.totals.value,
            "remaining_debt": report.totals.remaining_debt,
            "equity": report.totals.equity,
            "monthly_rent": report.totals.monthly_rent,
            "monthly_payments": report.totals.monthly_payments,
        },
    })
}

/// Libellé français d'une catégorie d'allocation.
fn allocation_label(kind: AccountKind) -> &'static str {
    match kind {
        AccountKind::Checking => "Comptes courants",
        AccountKind::Savings => "Épargne",
        AccountKind::Investment => "Investissements",
        AccountKind::Crypto => "Crypto",
        AccountKind::RealEstate => "Immobilier",
        AccountKind::Loan => "Dettes",
        AccountKind::Other => "Autres",
    }
}

fn totals(t: &Totals) -> Value {
    json!({ "gross_assets": t.gross_assets, "liabilities": t.liabilities, "net_worth": t.net_worth })
}

pub fn overview(o: &Overview) -> Value {
    with(
        totals(&o.totals),
        json!({
            "financial_assets": o.financial_assets,
            "invested": o.invested,
            "gain": o.gain,
            "allocation": o.allocation.iter().map(|slice| json!({
                "category": slice.kind.as_str(),
                "label": allocation_label(slice.kind),
                "value": slice.value,
                "pct": slice.pct,
            })).collect::<Vec<_>>(),
            "by_institution": o.by_institution.iter()
                .map(|(institution, value)| json!({ "institution": institution, "value": value })).collect::<Vec<_>>(),
            "by_envelope": o.by_envelope.iter()
                .map(|(envelope, value)| json!({ "envelope": envelope, "value": value })).collect::<Vec<_>>(),
            "members": o.members.iter().map(|m| with(
                json!({ "member_id": m.member.id.as_uuid(), "name": m.member.name, "color": m.member.color }),
                totals(&m.totals),
            )).collect::<Vec<_>>(),
            "top_holdings": o.top_holdings.iter().map(|h| json!({
                "name": h.name,
                "symbol": h.symbol,
                "value": h.value,
                "gain_pct": h.gain_pct,
                "account_name": h.account_name,
            })).collect::<Vec<_>>(),
            "weighted_expense_ratio": o.weighted_expense_ratio,
        }),
    )
}

fn snapshot_account(line: &AccountSnapshot) -> Value {
    json!({
        "account_id": line.account_id.as_uuid(),
        "name": line.name,
        "kind": line.kind.as_str(),
        "value": line.value,
        "gain": line.gain,
    })
}

pub fn history(points: &[SnapshotPoint]) -> Value {
    let points: Vec<Value> = points
        .iter()
        .map(|p| {
            let accounts = p
                .accounts
                .as_ref()
                .map(|lines| lines.iter().map(snapshot_account).collect::<Vec<_>>());
            with(
                json!({ "date": p.date, "accounts": accounts }),
                totals(&p.totals),
            )
        })
        .collect();
    json!({ "points": points })
}

pub fn reviewed_point(reviewed: &ReviewedPoint) -> Value {
    let p = &reviewed.point;
    with(
        json!({
            "date": p.date,
            "detailed": p.accounts.is_some(),
            "corrected": p.corrected,
            "change": reviewed.change,
            "suspicious": reviewed.suspicious,
        }),
        totals(&p.totals),
    )
}

pub fn history_review(review: &HistoryReview) -> Value {
    json!({
        "threshold": review.threshold.value(),
        "points": review.points.iter().map(reviewed_point).collect::<Vec<_>>(),
    })
}

pub fn price_history(points: &[PricePoint]) -> Value {
    let points: Vec<Value> = points
        .iter()
        .map(|p| json!({ "date": p.date, "price": p.price, "currency": p.currency.as_str() }))
        .collect();
    json!({ "points": points })
}

pub fn refresh_report(report: &RefreshReport) -> Value {
    json!({
        "updated": report.updated,
        "failed": report.failed.iter().map(|f| json!({
            "holding_id": f.holding_id.as_uuid(), "name": f.name, "reason": f.reason,
        })).collect::<Vec<_>>(),
    })
}

pub fn daily_report(report: &DailyReport) -> Value {
    json!({
        "prices": refresh_report(&report.prices),
        "snapshot_date": report.snapshot_date,
    })
}

pub fn goal(view: &GoalView) -> Value {
    let (g, p) = (&view.goal, &view.progress);
    json!({
        "id": g.id.as_uuid(),
        "name": g.name,
        "icon": g.icon,
        "kind": g.kind.as_str(),
        "target_amount": g.target_amount,
        "target_date": g.target_date,
        "monthly_contribution": g.monthly_contribution,
        "expected_return_pct": g.expected_return_pct,
        "monthly_expenses": g.monthly_expenses,
        "coverage_months": g.coverage_months,
        "account_ids": g.account_ids.iter().map(|id| id.as_uuid()).collect::<Vec<_>>(),
        "member_ids": g.member_ids.iter().map(|id| id.as_uuid()).collect::<Vec<_>>(),
        "progress": {
            "current_amount": p.current_amount,
            "target_amount": p.target_amount,
            "pct": p.pct,
            "status": p.status.as_str(),
            "required_return_pct": p.required_return_pct,
            "projected_amount_at_date": p.projected_amount_at_date,
            "coverage_months": p.coverage_months,
        },
    })
}

pub fn transaction(t: &Transaction) -> Value {
    json!({
        "id": t.id.as_uuid(),
        "account_id": t.account_id.as_uuid(),
        "date": t.date,
        "label": t.label,
        "amount": t.amount,
        "category": t.category,
    })
}

pub fn connection(c: &Connection) -> Value {
    json!({
        "id": c.id.as_uuid(),
        "provider": c.provider,
        "institution": c.institution,
        "status": c.status.as_str(),
        "last_sync_at": c.last_sync_at,
        "error": c.error,
    })
}

// ---------------------------------------------------------------- bundle (les valeurs nulles sont omises)

fn compact(fields: Value) -> Map<String, Value> {
    match fields {
        Value::Object(map) => map
            .into_iter()
            .filter(|(_, value)| !value.is_null())
            .collect(),
        _ => Map::new(),
    }
}

fn bundle_property(p: &PropertyInput) -> Value {
    Value::Object(compact(json!({
        "address": p.address,
        "category": p.category,
        "building_type": p.building_type,
        "surface_m2": p.surface_m2,
        "purchase_price": p.purchase_price,
        "purchase_fees": p.purchase_fees,
        "purchased_at": p.purchased_at,
        "current_value": p.current_value,
        "monthly_rent": p.monthly_rent,
        "monthly_charges": p.monthly_charges,
        "yearly_property_tax": p.yearly_property_tax,
    })))
}

fn bundle_loan(l: &LoanInput, property_external_id: Option<&str>) -> Value {
    Value::Object(compact(json!({
        "principal": l.principal,
        "interest_rate_pct": l.interest_rate_pct,
        "insurance_rate_pct": l.insurance_rate_pct,
        "duration_months": l.duration_months,
        "start_date": l.start_date,
        "property_external_id": property_external_id,
    })))
}

fn bundle_holding(h: &HoldingInput) -> Value {
    Value::Object(compact(json!({
        "kind": h.kind,
        "name": h.name,
        "isin": h.isin,
        "symbol": h.symbol,
        "security_type": h.security_type,
        "quantity": h.quantity,
        "buy_price": h.buy_price,
        "current_price": h.current_price,
        "currency": h.currency,
        "expense_ratio": h.expense_ratio,
        "price_source": h.price_source,
    })))
}

fn bundle_account_fields(a: &AccountInput) -> Map<String, Value> {
    compact(json!({
        "name": a.name,
        "institution": a.institution,
        "kind": a.kind,
        "envelope": a.envelope,
        "currency": a.currency,
        "cash_balance": a.cash_balance,
        "annual_yield_pct": a.annual_yield_pct,
        "opened_at": a.opened_at,
    }))
}

fn bundle_goal_fields(g: &GoalInput) -> Map<String, Value> {
    compact(json!({
        "name": g.name,
        "icon": g.icon,
        "kind": g.kind,
        "target_amount": g.target_amount,
        "target_date": g.target_date,
        "monthly_contribution": g.monthly_contribution,
        "expected_return_pct": g.expected_return_pct,
        "monthly_expenses": g.monthly_expenses,
        "coverage_months": g.coverage_months,
    }))
}

pub fn bundle(bundle: &Bundle) -> Value {
    let accounts: Vec<Value> = bundle
        .accounts
        .iter()
        .map(|entry| {
            let mut fields = bundle_account_fields(&entry.account);
            fields.insert("external_id".into(), json!(entry.external_id));
            let owners: Vec<Value> = entry
                .owners
                .iter()
                .map(|o| json!({ "member": o.member, "share": o.share }))
                .collect();
            fields.insert("owners".into(), Value::Array(owners));
            if let Some(holdings) = &entry.holdings {
                fields.insert(
                    "holdings".into(),
                    Value::Array(holdings.iter().map(bundle_holding).collect()),
                );
            }
            if let Some(property) = &entry.account.property {
                fields.insert("property".into(), bundle_property(property));
            }
            if let Some(loan) = &entry.account.loan {
                fields.insert(
                    "loan".into(),
                    bundle_loan(loan, entry.property_external_id.as_deref()),
                );
            }
            Value::Object(fields)
        })
        .collect();
    let goals: Vec<Value> = bundle
        .goals
        .iter()
        .map(|entry| {
            let mut fields = bundle_goal_fields(&entry.goal);
            fields.insert(
                "account_external_ids".into(),
                json!(entry.account_external_ids),
            );
            fields.insert("members".into(), json!(entry.members));
            Value::Object(fields)
        })
        .collect();
    json!({
        "version": bundle.version,
        "members": bundle.members.iter().map(|m| Value::Object(compact(json!({
            "name": m.name, "color": m.color, "is_primary": m.is_primary,
        })))).collect::<Vec<_>>(),
        "accounts": accounts,
        "goals": goals,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fineme_application::bundle::{BundleAccount, BundleMember, BundleOwner};

    #[test]
    fn compact_drops_nulls_from_absent_and_cleared_fields() {
        let fields = compact(
            json!({ "a": 1, "b": Option::<f64>::None, "c": Some(Option::<String>::None), "d": Some(Some("x")) }),
        );
        assert_eq!(Value::Object(fields), json!({ "a": 1, "d": "x" }));
    }

    #[test]
    fn with_merges_objects() {
        assert_eq!(
            with(json!({ "a": 1 }), json!({ "b": 2 })),
            json!({ "a": 1, "b": 2 })
        );
    }

    #[test]
    fn bundle_json_matches_the_contract_shape() {
        let exported = Bundle {
            version: 1,
            members: vec![BundleMember {
                name: "Alice".into(),
                color: Some("#6366f1".into()),
                is_primary: Some(true),
            }],
            accounts: vec![BundleAccount {
                external_id: "pret-1".into(),
                account: AccountInput {
                    name: Some("Prêt".into()),
                    institution: Some(None),
                    kind: Some("loan".into()),
                    cash_balance: Some(0.0),
                    loan: Some(LoanInput {
                        principal: Some(1000.0),
                        duration_months: Some(12),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                owners: vec![BundleOwner {
                    member: "Alice".into(),
                    share: 0.5,
                }],
                holdings: None,
                property_external_id: Some("maison-1".into()),
            }],
            goals: vec![],
        };
        assert_eq!(
            bundle(&exported),
            json!({
                "version": 1,
                "members": [{ "name": "Alice", "color": "#6366f1", "is_primary": true }],
                "accounts": [{
                    "external_id": "pret-1", "name": "Prêt", "kind": "loan", "cash_balance": 0.0,
                    "owners": [{ "member": "Alice", "share": 0.5 }],
                    "loan": { "principal": 1000.0, "duration_months": 12, "property_external_id": "maison-1" },
                }],
                "goals": [],
            })
        );
    }

    #[test]
    fn every_asset_kind_has_a_french_label() {
        for kind in AccountKind::ALL {
            assert!(!allocation_label(*kind).is_empty());
        }
        assert_eq!(allocation_label(AccountKind::RealEstate), "Immobilier");
    }
}
