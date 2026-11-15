//! Routes de l'API : chaque handler traduit la requête en appel de cas d'usage puis la réponse en JSON.

use axum::extract::State;
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Extension, Router, middleware};
use fineme_application::bundle::Bundle;
use fineme_application::commands::{HoldingInput, TransactionInput};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthUser, require_auth};
use crate::error::{ApiError, ApiResult, Json, Path, Query};
use crate::input::{
    AccountBody, BundleBody, ConnectionBody, GoalBody, HistoryQuery, HoldingBody, HoldingsBulkBody,
    LoginBody, MemberBody, PageQuery, SetupBody, TransactionsBulkBody, ViewQuery,
};
use crate::output;

type JsonResponse = ApiResult<axum::Json<Value>>;
type Created = ApiResult<(StatusCode, axum::Json<Value>)>;

fn ok(value: Value) -> JsonResponse {
    Ok(axum::Json(value))
}

fn created(value: Value) -> Created {
    Ok((StatusCode::CREATED, axum::Json(value)))
}

pub fn api(app: AppState) -> Router<AppState> {
    let public = Router::new()
        .route("/health", get(health))
        .route("/auth/status", get(auth_status))
        .route("/auth/setup", post(auth_setup))
        .route("/auth/login", post(auth_login));

    let protected = Router::new()
        .route("/auth/me", get(auth_me))
        .route("/members", get(members_list).post(members_create))
        .route("/members/{id}", put(members_update).delete(members_delete))
        .route("/accounts", get(accounts_list).post(accounts_create))
        .route(
            "/accounts/{id}",
            get(accounts_get)
                .put(accounts_update)
                .delete(accounts_delete),
        )
        .route("/accounts/{id}/holdings", post(holdings_create))
        .route("/accounts/{id}/holdings/bulk", post(holdings_bulk))
        .route(
            "/holdings/{id}",
            put(holdings_update).delete(holdings_delete),
        )
        .route("/prices/refresh", post(prices_refresh))
        .route("/fx", get(fx))
        .route("/accounts/{id}/transactions", get(transactions_list))
        .route("/accounts/{id}/transactions/bulk", post(transactions_bulk))
        .route("/transactions/{id}", delete(transactions_delete))
        .route("/real-estate", get(real_estate))
        .route("/loans/{account_id}/schedule", get(loan_schedule))
        .route("/overview", get(overview))
        .route("/history", get(history))
        .route("/goals", get(goals_list).post(goals_create))
        .route("/goals/{id}", put(goals_update).delete(goals_delete))
        .route("/connections/providers", get(connection_providers))
        .route(
            "/connections",
            get(connections_list).post(connections_create),
        )
        .route("/connections/{id}/sync", post(connections_sync))
        .route("/connections/{id}", delete(connections_delete))
        .route("/export", get(export))
        .route("/import", post(import))
        .route_layer(middleware::from_fn_with_state(app, require_auth));

    public.merge(protected)
}

pub async fn not_found() -> ApiError {
    ApiError::new(StatusCode::NOT_FOUND, "Route introuvable")
}

pub async fn method_not_allowed() -> ApiError {
    ApiError::new(StatusCode::METHOD_NOT_ALLOWED, "Méthode non autorisée")
}

// ---------------------------------------------------------------- santé et authentification

async fn health() -> axum::Json<Value> {
    axum::Json(json!({ "status": "ok" }))
}

async fn auth_status(State(app): State<AppState>) -> JsonResponse {
    ok(json!({ "setup_required": app.auth.setup_required().await? }))
}

async fn auth_setup(State(app): State<AppState>, Json(body): Json<SetupBody>) -> Created {
    let session = app
        .auth
        .setup(&body.email, &body.password, &body.name)
        .await?;
    created(json!({ "token": session.token, "user": output::user(&session.user) }))
}

async fn auth_login(State(app): State<AppState>, Json(body): Json<LoginBody>) -> JsonResponse {
    let session = app.auth.login(&body.email, &body.password).await?;
    ok(json!({ "token": session.token, "user": output::user(&session.user) }))
}

async fn auth_me(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
) -> JsonResponse {
    ok(output::user(&app.auth.me(id).await?))
}

// ---------------------------------------------------------------- membres

async fn members_list(State(app): State<AppState>) -> JsonResponse {
    ok(app
        .members
        .list()
        .await?
        .iter()
        .map(output::member)
        .collect())
}

async fn members_create(State(app): State<AppState>, Json(body): Json<MemberBody>) -> Created {
    created(output::member(
        &app.members.create(body.name, body.color).await?,
    ))
}

async fn members_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<MemberBody>,
) -> JsonResponse {
    ok(output::member(
        &app.members.update(id, body.name, body.color).await?,
    ))
}

async fn members_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.members.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- comptes

async fn accounts_list(State(app): State<AppState>, Query(view): Query<ViewQuery>) -> JsonResponse {
    ok(app
        .accounts
        .list(view.member_id)
        .await?
        .iter()
        .map(output::account)
        .collect())
}

async fn accounts_get(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Query(view): Query<ViewQuery>,
) -> JsonResponse {
    ok(output::account_detail(
        &app.accounts.get(id, view.member_id).await?,
    ))
}

async fn accounts_create(State(app): State<AppState>, Json(body): Json<AccountBody>) -> Created {
    created(output::account_detail(
        &app.accounts.create(&body.into()).await?,
    ))
}

async fn accounts_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AccountBody>,
) -> JsonResponse {
    ok(output::account_detail(
        &app.accounts.update(id, &body.into()).await?,
    ))
}

async fn accounts_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.accounts.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- positions, cours, change

async fn holdings_create(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<HoldingBody>,
) -> Created {
    created(output::holding(
        &app.holdings.create(id, &body.into()).await?,
    ))
}

async fn holdings_bulk(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<HoldingsBulkBody>,
) -> JsonResponse {
    let inputs: Vec<HoldingInput> = body.holdings.into_iter().map(HoldingInput::from).collect();
    ok(app
        .holdings
        .bulk(id, &inputs, body.replace)
        .await?
        .iter()
        .map(output::holding)
        .collect())
}

async fn holdings_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<HoldingBody>,
) -> JsonResponse {
    ok(output::holding(
        &app.holdings.update(id, &body.into()).await?,
    ))
}

async fn holdings_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.holdings.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn prices_refresh(State(app): State<AppState>) -> JsonResponse {
    let report = app.prices.refresh().await?;
    ok(json!({
        "updated": report.updated,
        "failed": report.failed.iter().map(|f| json!({
            "holding_id": f.holding_id.as_uuid(), "name": f.name, "reason": f.reason,
        })).collect::<Vec<_>>(),
    }))
}

async fn fx(State(app): State<AppState>) -> JsonResponse {
    let (rates, updated_at) = app.prices.fx().await?;
    let rates: serde_json::Map<String, Value> = rates
        .iter()
        .map(|(currency, rate)| (currency.as_str().to_string(), json!(rate)))
        .collect();
    ok(json!({ "base": "EUR", "rates": rates, "updated_at": updated_at }))
}

// ---------------------------------------------------------------- transactions

async fn transactions_list(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Query(page): Query<PageQuery>,
) -> JsonResponse {
    let page = app.transactions.list(id, page.limit, page.offset).await?;
    ok(
        json!({ "items": page.items.iter().map(output::transaction).collect::<Vec<_>>(), "total": page.total }),
    )
}

async fn transactions_bulk(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TransactionsBulkBody>,
) -> JsonResponse {
    let inputs: Vec<TransactionInput> = body
        .transactions
        .into_iter()
        .map(TransactionInput::from)
        .collect();
    let result = app
        .transactions
        .bulk(id, inputs, body.new_cash_balance)
        .await?;
    ok(json!({ "inserted": result.inserted, "skipped": result.skipped }))
}

async fn transactions_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.transactions.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- immobilier, vue d'ensemble

async fn real_estate(State(app): State<AppState>, Query(view): Query<ViewQuery>) -> JsonResponse {
    ok(output::real_estate(
        &app.real_estate.report(view.member_id).await?,
    ))
}

async fn loan_schedule(State(app): State<AppState>, Path(account_id): Path<Uuid>) -> JsonResponse {
    let (loan, rows) = app.real_estate.loan_schedule(account_id).await?;
    ok(output::schedule(&loan, &rows))
}

async fn overview(State(app): State<AppState>, Query(view): Query<ViewQuery>) -> JsonResponse {
    ok(output::overview(
        &app.overview.overview(view.member_id).await?,
    ))
}

async fn history(State(app): State<AppState>, Query(query): Query<HistoryQuery>) -> JsonResponse {
    ok(output::history(
        &app.overview
            .history(query.range.as_deref(), query.member_id)
            .await?,
    ))
}

// ---------------------------------------------------------------- objectifs

async fn goals_list(State(app): State<AppState>) -> JsonResponse {
    ok(app.goals.list().await?.iter().map(output::goal).collect())
}

async fn goals_create(State(app): State<AppState>, Json(body): Json<GoalBody>) -> Created {
    created(output::goal(&app.goals.create(&body.into()).await?))
}

async fn goals_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<GoalBody>,
) -> JsonResponse {
    ok(output::goal(&app.goals.update(id, &body.into()).await?))
}

async fn goals_delete(State(app): State<AppState>, Path(id): Path<Uuid>) -> ApiResult<StatusCode> {
    app.goals.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- connexions bancaires

async fn connection_providers(State(app): State<AppState>) -> axum::Json<Value> {
    let providers = app.connections.providers();
    axum::Json(
        providers
            .iter()
            .map(|p| json!({ "id": p.id, "name": p.name, "configured": p.configured }))
            .collect(),
    )
}

async fn connections_list(State(app): State<AppState>) -> JsonResponse {
    ok(app
        .connections
        .list()
        .await?
        .iter()
        .map(output::connection)
        .collect())
}

async fn connections_create(
    State(app): State<AppState>,
    Json(body): Json<ConnectionBody>,
) -> Created {
    let started = app.connections.create(&body.provider).await?;
    created(json!({ "id": started.id.as_uuid(), "redirect_url": started.redirect_url }))
}

async fn connections_sync(State(app): State<AppState>, Path(id): Path<Uuid>) -> JsonResponse {
    let report = app.connections.sync(id).await?;
    ok(
        json!({ "accounts_updated": report.accounts_updated, "transactions_inserted": report.transactions_inserted }),
    )
}

async fn connections_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.connections.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- import / export

async fn export(State(app): State<AppState>) -> JsonResponse {
    ok(output::bundle(&app.bundle.export().await?))
}

async fn import(State(app): State<AppState>, Json(body): Json<BundleBody>) -> JsonResponse {
    let counts = app.bundle.import(Bundle::from(body)).await?;
    ok(json!({
        "members": counts.members,
        "accounts": counts.accounts,
        "holdings": counts.holdings,
        "goals": counts.goals,
    }))
}
