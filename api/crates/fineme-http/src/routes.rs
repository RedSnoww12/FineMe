//! Routes de l'API : chaque handler traduit la requête en appel de cas d'usage puis la réponse en JSON.

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Router, middleware};
use chrono::NaiveDate;
use fineme_application::auth::Session;
use fineme_application::bundle::Bundle;
use fineme_application::commands::{
    HoldingInput, OperationImportInput, OperationInput, TransactionInput, TransactionPatch,
    ValuationInput,
};
use fineme_application::totp::LoginOutcome;
use fineme_domain::ids::{AccessTokenId, AccountId};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;
use crate::auth::{AuthUser, bearer_token, require_auth};
use crate::error::{ApiError, ApiResult, Json, Path, Query};
use crate::input::{
    AccessTokenCreateBody, AccessTokenRevokeBody, AccountBody, AlertPatchBody, AlertRuleBody,
    AlertsQuery, BenchmarkBody, BudgetQuery, BudgetTargetBody, BundleBody, CompareQuery,
    ConnectionBody, DateRangeQuery, GoalBody, HistoryCorrectionBody, HistoryQuery,
    HistoryResetBody, HistoryReviewQuery, HoldingBody, HoldingsBulkBody, ImportProfileBody,
    LimitQuery, LoginBody, LoginTotpBody, MemberBody, OperationBody, OperationsBulkBody, PageQuery,
    PasswordChangeBody, PasswordResetBody, PerformanceQuery, RangeQuery, SetupBody, TotpCodeBody,
    TotpDisableBody, TransactionPatchBody, TransactionsBulkBody, ValuationBody, ViewQuery,
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
        .route("/auth/login/totp", post(auth_login_totp))
        .route("/auth/reset", post(auth_reset))
        // Protégée par CRON_SECRET et non par le JWT : appelée par un planificateur, sans session.
        .route("/jobs/daily", post(jobs_daily));

    let protected = Router::new()
        .route("/auth/me", get(auth_me))
        .route("/auth/password", put(auth_password))
        .route("/auth/refresh", post(auth_refresh))
        .route("/auth/logins", get(auth_logins))
        .route("/auth/totp", get(totp_status))
        .route("/auth/totp/setup", post(totp_setup))
        .route("/auth/totp/enable", post(totp_enable))
        .route("/auth/totp/disable", post(totp_disable))
        .route("/auth/totp/recovery-codes", post(totp_recovery_codes))
        .route(
            "/access-tokens",
            get(access_tokens_list).post(access_tokens_create),
        )
        .route("/access-tokens/calls", get(access_tokens_calls))
        .route("/access-tokens/{id}/revoke", post(access_tokens_revoke))
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
        .route(
            "/transactions/{id}",
            put(transactions_update).delete(transactions_delete),
        )
        .route("/budget", get(budget_month))
        .route("/budget/transactions", get(budget_transactions))
        .route("/budget/categories", get(budget_categories))
        .route("/budget/rules", get(budget_rules))
        .route("/budget/rules/{id}", delete(budget_rule_delete))
        .route(
            "/budget/targets",
            get(budget_targets).put(budget_target_set),
        )
        .route(
            "/accounts/{id}/operations",
            get(operations_list).post(operations_create),
        )
        .route("/accounts/{id}/operations/bulk", post(operations_bulk))
        .route("/accounts/{id}/operations/summary", get(operations_summary))
        .route("/accounts/{id}/import-profiles", get(import_profiles_list))
        .route(
            "/accounts/{id}/import-profiles/{kind}",
            put(import_profiles_save),
        )
        .route(
            "/operations/{id}",
            put(operations_update).delete(operations_delete),
        )
        .route("/real-estate", get(real_estate))
        .route(
            "/accounts/{id}/valuations",
            get(valuations_list).post(valuations_create),
        )
        .route(
            "/valuations/{id}",
            put(valuations_update).delete(valuations_delete),
        )
        .route("/loans/{account_id}/schedule", get(loan_schedule))
        .route("/overview", get(overview))
        .route("/history", get(history))
        .route("/history/compare", get(history_compare))
        .route(
            "/history/points",
            get(history_review).delete(history_delete),
        )
        .route("/history/points/{date}", put(history_correct))
        .route("/history/recompute", post(history_recompute))
        .route("/history/reset", post(history_reset))
        .route("/performance", get(performance))
        .route(
            "/performance/benchmark",
            get(benchmark_get)
                .put(benchmark_put)
                .delete(benchmark_delete),
        )
        .route("/goals", get(goals_list).post(goals_create))
        .route("/goals/{id}", put(goals_update).delete(goals_delete))
        .route("/alerts", get(alerts_list))
        .route("/alerts/evaluate", post(alerts_evaluate))
        .route("/alerts/read-all", post(alerts_read_all))
        .route("/alerts/rules", get(alert_rules))
        .route("/alerts/rules/{kind}", put(alert_rule_update))
        .route("/alerts/{id}", put(alerts_update))
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
    match app.auth.login(&body.email, &body.password, &origin).await? {
        LoginOutcome::Session(session) => ok(self::session(&session)),
        LoginOutcome::Challenge { token, expires_in } => ok(json!({
            "totp_required": true,
            "challenge": token,
            "expires_in": expires_in,
        })),
    }
}

async fn auth_login_totp(
    State(app): State<AppState>,
    ClientOrigin(origin): ClientOrigin,
    Json(body): Json<LoginTotpBody>,
) -> JsonResponse {
    let session = app
        .auth
        .login_totp(&body.challenge, &body.code, &origin)
        .await?;
    ok(self::session(&session))
}

// ---------------------------------------------------------------- second facteur

async fn totp_status(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
) -> JsonResponse {
    let status = app.auth.totp_status(id).await?;
    ok(json!({
        "enabled": status.enabled,
        "recovery_codes_remaining": status.recovery_codes_remaining,
    }))
}

async fn totp_setup(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
) -> JsonResponse {
    let enrollment = app.auth.start_totp(id).await?;
    ok(json!({
        "secret": enrollment.secret,
        "otpauth_uri": enrollment.otpauth_uri,
        "qr_svg": enrollment.qr_svg,
    }))
}

async fn totp_enable(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
    Json(body): Json<TotpCodeBody>,
) -> JsonResponse {
    let activation = app.auth.enable_totp(id, &body.code).await?;
    let mut response = self::session(&activation.session);
    response["recovery_codes"] = json!(activation.recovery_codes);
    ok(response)
}

async fn totp_disable(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
    ClientOrigin(origin): ClientOrigin,
    Json(body): Json<TotpDisableBody>,
) -> JsonResponse {
    let session = app
        .auth
        .disable_totp(id, &body.password, &body.code, &origin)
        .await?;
    ok(self::session(&session))
}

async fn totp_recovery_codes(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
    ClientOrigin(origin): ClientOrigin,
    Json(body): Json<TotpCodeBody>,
) -> JsonResponse {
    let codes = app
        .auth
        .regenerate_recovery_codes(id, &body.code, &origin)
        .await?;
    ok(json!({ "recovery_codes": codes }))
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

async fn access_tokens_list(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
) -> JsonResponse {
    ok(app
        .auth
        .access_tokens(id)
        .await?
        .iter()
        .map(output::access_token)
        .collect())
}

async fn access_tokens_create(
    State(app): State<AppState>,
    Extension(AuthUser(id)): Extension<AuthUser>,
    ClientOrigin(origin): ClientOrigin,
    Json(body): Json<AccessTokenCreateBody>,
) -> ApiResult<impl axum::response::IntoResponse> {
    let token = app
        .auth
        .create_access_token(
            id,
            &body.name,
            &body.password,
            body.code.as_deref(),
            &origin,
        )
        .await?;
    // Le secret n'est affiché qu'une fois : aucune mise en cache de cette réponse.
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        axum::Json(output::created_access_token(&token)),
    ))
}

async fn access_tokens_revoke(
    State(app): State<AppState>,
    Extension(AuthUser(user)): Extension<AuthUser>,
    ClientOrigin(origin): ClientOrigin,
    Path(id): Path<Uuid>,
    Json(body): Json<AccessTokenRevokeBody>,
) -> JsonResponse {
    let token = app
        .auth
        .revoke_access_token(
            user,
            AccessTokenId::from_uuid(id),
            &body.password,
            body.code.as_deref(),
            &origin,
        )
        .await?;
    ok(output::access_token(&token))
}

async fn access_tokens_calls(
    State(app): State<AppState>,
    Query(query): Query<LimitQuery>,
) -> JsonResponse {
    ok(app
        .auth
        .mcp_calls(query.limit)
        .await?
        .iter()
        .map(output::mcp_call)
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

async fn transactions_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TransactionPatchBody>,
) -> JsonResponse {
    let transaction = app
        .transactions
        .update(id, TransactionPatch::from(body))
        .await?;
    ok(output::transaction(&transaction))
}

async fn transactions_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.transactions.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- budget

async fn budget_month(State(app): State<AppState>, Query(q): Query<BudgetQuery>) -> JsonResponse {
    ok(output::budget_month(
        &app.budget.month(q.month.as_deref(), q.member_id).await?,
    ))
}

async fn budget_transactions(
    State(app): State<AppState>,
    Query(q): Query<BudgetQuery>,
) -> JsonResponse {
    let items = app
        .budget
        .transactions(q.month.as_deref(), q.member_id)
        .await?;
    ok(Value::Array(
        items.iter().map(output::budget_transaction).collect(),
    ))
}

async fn budget_categories(State(app): State<AppState>) -> JsonResponse {
    ok(json!(app.budget.categories().await?))
}

async fn budget_rules(State(app): State<AppState>) -> JsonResponse {
    let rules = app.budget.rules().await?;
    ok(Value::Array(
        rules.iter().map(output::budget_rule).collect(),
    ))
}

async fn budget_rule_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.budget.delete_rule(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn budget_targets(State(app): State<AppState>) -> JsonResponse {
    let targets = app.budget.targets().await?;
    ok(Value::Array(
        targets.iter().map(output::budget_target).collect(),
    ))
}

async fn budget_target_set(
    State(app): State<AppState>,
    Json(body): Json<BudgetTargetBody>,
) -> JsonResponse {
    let targets = app
        .budget
        .set_target(&body.category, body.monthly_amount)
        .await?;
    ok(Value::Array(
        targets.iter().map(output::budget_target).collect(),
    ))
}

// ---------------------------------------------------------------- journal d'opérations

async fn operations_list(State(app): State<AppState>, Path(id): Path<Uuid>) -> JsonResponse {
    ok(app
        .operations
        .list(id)
        .await?
        .iter()
        .map(output::operation)
        .collect())
}

async fn operations_summary(State(app): State<AppState>, Path(id): Path<Uuid>) -> JsonResponse {
    ok(output::operations_summary(
        &app.operations.summary(id).await?,
    ))
}

async fn operations_create(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<OperationBody>,
) -> Created {
    created(output::operation(
        &app.operations
            .create(id, &OperationInput::from(body))
            .await?,
    ))
}

async fn operations_bulk(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<OperationsBulkBody>,
) -> JsonResponse {
    let inputs: Vec<OperationImportInput> = body
        .operations
        .into_iter()
        .map(OperationImportInput::from)
        .collect();
    ok(output::operations_import(
        &app.operations.import(id, &inputs).await?,
    ))
}

async fn import_profiles_list(State(app): State<AppState>, Path(id): Path<Uuid>) -> JsonResponse {
    ok(app
        .import_profiles
        .list(id)
        .await?
        .iter()
        .map(output::import_profile)
        .collect())
}

async fn import_profiles_save(
    State(app): State<AppState>,
    Path((id, kind)): Path<(Uuid, String)>,
    Json(body): Json<ImportProfileBody>,
) -> JsonResponse {
    ok(output::import_profile(
        &app.import_profiles
            .save(id, &kind, body.template, body.columns)
            .await?,
    ))
}

async fn operations_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<OperationBody>,
) -> JsonResponse {
    ok(output::operation(
        &app.operations
            .update(id, &OperationInput::from(body))
            .await?,
    ))
}

async fn operations_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.operations.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- immobilier, vue d'ensemble

async fn valuations_list(State(app): State<AppState>, Path(id): Path<Uuid>) -> JsonResponse {
    let account_id = AccountId::from_uuid(id);
    ok(app
        .real_estate
        .valuations(id)
        .await?
        .iter()
        .map(|v| output::valuation(account_id, v))
        .collect())
}

async fn valuations_create(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ValuationBody>,
) -> Created {
    let saved = app
        .real_estate
        .add_valuation(id, &ValuationInput::from(body))
        .await?;
    created(output::valuation(saved.account_id, &saved.valuation))
}

async fn valuations_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ValuationBody>,
) -> JsonResponse {
    let saved = app
        .real_estate
        .update_valuation(id, &ValuationInput::from(body))
        .await?;
    ok(output::valuation(saved.account_id, &saved.valuation))
}

async fn valuations_delete(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<StatusCode> {
    app.real_estate.delete_valuation(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

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

async fn history_compare(
    State(app): State<AppState>,
    Query(query): Query<CompareQuery>,
) -> JsonResponse {
    let a = query
        .a
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Date « a » requise (AAAA-MM-JJ)"))?;
    ok(output::history_comparison(
        &app.overview.compare(query.member_id, a, query.b).await?,
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

// ---------------------------------------------------------------- performance

async fn performance(
    State(app): State<AppState>,
    Query(query): Query<PerformanceQuery>,
) -> JsonResponse {
    ok(output::performance(
        &app.performance
            .performance(query.member_id, query.from, query.to)
            .await?,
    ))
}

async fn benchmark_get(State(app): State<AppState>) -> JsonResponse {
    ok(app
        .performance
        .benchmark()
        .await?
        .as_ref()
        .map_or(Value::Null, output::benchmark_info))
}

async fn benchmark_put(
    State(app): State<AppState>,
    Json(body): Json<BenchmarkBody>,
) -> JsonResponse {
    ok(output::benchmark_info(
        &app.performance.set_benchmark(&body.into()).await?,
    ))
}

async fn benchmark_delete(State(app): State<AppState>) -> ApiResult<StatusCode> {
    app.performance.delete_benchmark().await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- objectifs

// ---------------------------------------------------------------- alertes

async fn alerts_list(
    State(app): State<AppState>,
    Query(query): Query<AlertsQuery>,
) -> JsonResponse {
    let alerts = app.alerts.list(query.archived).await?;
    ok(Value::Array(alerts.iter().map(output::alert).collect()))
}

async fn alerts_evaluate(State(app): State<AppState>) -> JsonResponse {
    ok(output::evaluation_report(&app.alerts.evaluate().await?))
}

async fn alerts_read_all(State(app): State<AppState>) -> JsonResponse {
    ok(json!({ "updated": app.alerts.read_all().await? }))
}

async fn alerts_update(
    State(app): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AlertPatchBody>,
) -> JsonResponse {
    ok(output::alert(&app.alerts.update(id, body.into()).await?))
}

async fn alert_rules(State(app): State<AppState>) -> JsonResponse {
    let rules = app.alerts.rules().await?;
    ok(Value::Array(rules.iter().map(output::alert_rule).collect()))
}

async fn alert_rule_update(
    State(app): State<AppState>,
    Path(kind): Path<String>,
    Json(body): Json<AlertRuleBody>,
) -> JsonResponse {
    ok(output::alert_rule(
        &app.alerts.update_rule(&kind, body.into()).await?,
    ))
}

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
