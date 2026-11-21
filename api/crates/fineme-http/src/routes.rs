//! Routes de l'API : chaque handler traduit la requête en appel de cas d'usage puis la réponse en JSON.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Router, middleware};
use chrono::NaiveDate;
use fineme_application::auth::Session;
use fineme_application::bundle::Bundle;
use fineme_application::commands::{HoldingInput, TransactionInput};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthUser, bearer_token, require_auth};
use crate::error::{ApiError, ApiResult, Json, Path, Query};
use crate::input::{
    AccountBody, BundleBody, ConnectionBody, DateRangeQuery, GoalBody, HistoryCorrectionBody,
    HistoryQuery, HistoryResetBody, HistoryReviewQuery, HoldingBody, HoldingsBulkBody, LimitQuery,
    LoginBody, MemberBody, PageQuery, PasswordChangeBody, PasswordResetBody, RangeQuery, SetupBody,
    TransactionsBulkBody, ViewQuery,
};
use crate::origin::ClientOrigin;
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
        .route("/auth/login", post(auth_login))
        .route("/auth/reset", post(auth_reset))
        // Protégée par CRON_SECRET et non par le JWT : appelée par un planificateur, sans session.
        .route("/jobs/daily", post(jobs_daily));

    let protected = Router::new()
        .route("/auth/me", get(auth_me))
        .route("/auth/password", put(auth_password))
        .route("/auth/refresh", post(auth_refresh))
        .route("/auth/logins", get(auth_logins))
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
        .route("/holdings/{id}/prices", get(holdings_prices))
        .route("/prices/refresh", post(prices_refresh))
        .route("/fx", get(fx))
        .route("/accounts/{id}/transactions", get(transactions_list))
        .route("/accounts/{id}/transactions/bulk", post(transactions_bulk))
        .route("/transactions/{id}", delete(transactions_delete))
        .route("/real-estate", get(real_estate))
        .route("/loans/{account_id}/schedule", get(loan_schedule))
        .route("/overview", get(overview))
        .route("/history", get(history))
        .route(
            "/history/points",
            get(history_review).delete(history_delete),
        )
        .route("/history/points/{date}", put(history_correct))
        .route("/history/recompute", post(history_recompute))
        .route("/history/reset", post(history_reset))
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
    ok(json!({
        "setup_required": app.auth.setup_required().await?,
        "setup_token_required": app.auth.setup_token_required(),
        "recovery_enabled": app.auth.recovery_enabled(),
    }))
}

async fn auth_setup(State(app): State<AppState>, Json(body): Json<SetupBody>) -> Created {
    let session = app
        .auth
        .setup(
            &body.email,
            &body.password,
            &body.name,
            body.setup_token.as_deref(),
        )
        .await?;
    created(self::session(&session))
}

async fn auth_login(
    State(app): State<AppState>,
    ClientOrigin(origin): ClientOrigin,
    Json(body): Json<LoginBody>,
) -> JsonResponse {
    let session = app.auth.login(&body.email, &body.password, &origin).await?;
    ok(self::session(&session))
}

async fn auth_refresh(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
) -> JsonResponse {
    ok(self::session(&app.auth.refresh(id).await?))
}

async fn auth_logins(State(app): State<AppState>, Query(query): Query<LimitQuery>) -> JsonResponse {
    ok(app
        .auth
        .login_history(query.limit)
        .await?
        .iter()
        .map(output::login_event)
        .collect())
}

fn session(session: &Session) -> Value {
    json!({ "token": session.token, "user": output::user(&session.user) })
}

async fn auth_reset(
    State(app): State<AppState>,
    ClientOrigin(origin): ClientOrigin,
    Json(body): Json<PasswordResetBody>,
) -> JsonResponse {
    let session = app
        .auth
        .reset_password(
            &body.email,
            body.recovery_token.as_deref(),
            &body.new_password,
            &origin,
        )
        .await?;
    ok(self::session(&session))
}

async fn auth_password(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
    Json(body): Json<PasswordChangeBody>,
) -> JsonResponse {
    let session = app
        .auth
        .change_password(id, &body.current_password, &body.new_password)
        .await?;
    ok(self::session(&session))
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

async fn holdings_prices(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<RangeQuery>,
) -> JsonResponse {
    ok(output::price_history(
        &app.prices.history(id, query.range.as_deref()).await?,
    ))
}

async fn prices_refresh(State(app): State<AppState>) -> JsonResponse {
    ok(output::refresh_report(&app.prices.refresh().await?))
}

// ---------------------------------------------------------------- tâche quotidienne

async fn jobs_daily(State(app): State<AppState>, headers: HeaderMap) -> JsonResponse {
    let secret = bearer_token(
        headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok()),
    );
    ok(output::daily_report(&app.run_daily(secret).await?))
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

// ---------------------------------------------------------------- nettoyage de l'historique

async fn history_review(
    State(app): State<AppState>,
    Query(query): Query<HistoryReviewQuery>,
) -> JsonResponse {
    ok(output::history_review(
        &app.history.review(query.member_id, query.threshold).await?,
    ))
}

async fn history_correct(
    State(app): State<AppState>,
    Path(date): Path<String>,
    Query(view): Query<ViewQuery>,
    Json(body): Json<HistoryCorrectionBody>,
) -> JsonResponse {
    let date = NaiveDate::parse_from_str(&date, "%Y-%m-%d").map_err(|_| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "Date invalide dans l'URL (AAAA-MM-JJ attendu)",
        )
    })?;
    let point = app
        .history
        .correct(date, view.member_id, body.gross_assets, body.liabilities)
        .await?;
    ok(output::reviewed_point(&point))
}

async fn history_delete(
    State(app): State<AppState>,
    Query(range): Query<DateRangeQuery>,
) -> JsonResponse {
    let deleted_days = app.history.delete(range.from, range.to).await?;
    ok(json!({ "deleted_days": deleted_days }))
}

async fn history_recompute(State(app): State<AppState>) -> JsonResponse {
    let snapshot_date = app.overview.record_snapshots().await?;
    ok(json!({ "snapshot_date": snapshot_date }))
}

async fn history_reset(
    State(app): State<AppState>,
    Json(body): Json<HistoryResetBody>,
) -> JsonResponse {
    let deleted_days = app.history.reset(body.confirmation.as_deref()).await?;
    ok(json!({ "deleted_days": deleted_days }))
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
