use std::sync::Arc;

use fineme_domain::goal::{Goal, GoalKind, GoalProgress};
use fineme_domain::ids::{AccountId, GoalId, MemberId};
use fineme_domain::portfolio::Portfolio;
use uuid::Uuid;

use crate::commands::{GoalInput, clean};
use crate::error::{AppError, AppResult};
use crate::ports::GoalRepository;
use crate::reader::PortfolioReader;

#[derive(Debug, Clone, PartialEq)]
pub struct GoalView {
    pub goal: Goal,
    pub progress: GoalProgress,
}

fn unique<T: PartialEq + Copy>(ids: impl IntoIterator<Item = T>) -> Vec<T> {
    let mut seen = Vec::new();
    for id in ids {
        if !seen.contains(&id) {
            seen.push(id);
        }
    }
    seen
}

/// Construit un objectif à partir de l'entrée : création (`existing = None`) ou mise à jour partielle.
/// Les comptes et membres liés doivent exister.
pub(crate) fn build_goal(
    existing: Option<&Goal>,
    input: &GoalInput,
    portfolio: &Portfolio,
) -> AppResult<Goal> {
    let kind = match (&input.kind, existing) {
        (Some(kind), _) => GoalKind::parse(kind)?,
        (None, Some(goal)) => goal.kind,
        (None, None) => return Err(AppError::invalid("Le type d'objectif (kind) est requis")),
    };
    let mut goal = existing.cloned().unwrap_or_else(|| Goal {
        id: GoalId::generate(),
        name: String::new(),
        icon: None,
        kind,
        target_amount: None,
        target_date: None,
        monthly_contribution: None,
        expected_return_pct: None,
        monthly_expenses: None,
        coverage_months: None,
        account_ids: Vec::new(),
        member_ids: Vec::new(),
    });
    goal.kind = kind;
    if let Some(name) = &input.name {
        goal.name = name.trim().to_string();
    }
    if let Some(icon) = &input.icon {
        goal.icon = clean(icon.clone());
    }
    if let Some(target_date) = input.target_date {
        goal.target_date = target_date;
    }
    for (target, value) in [
        (&mut goal.target_amount, input.target_amount),
        (&mut goal.monthly_contribution, input.monthly_contribution),
        (&mut goal.expected_return_pct, input.expected_return_pct),
        (&mut goal.monthly_expenses, input.monthly_expenses),
        (&mut goal.coverage_months, input.coverage_months),
    ] {
        if let Some(value) = value {
            *target = value;
        }
    }
    goal.check()?;
    if let Some(ids) = &input.account_ids {
        goal.account_ids = unique(ids.iter().map(|id| AccountId::from_uuid(*id)));
        if goal
            .account_ids
            .iter()
            .any(|id| portfolio.account(*id).is_none())
        {
            return Err(AppError::invalid("Un des comptes liés est introuvable"));
        }
    }
    if let Some(ids) = &input.member_ids {
        goal.member_ids = unique(ids.iter().map(|id| MemberId::from_uuid(*id)));
        if goal
            .member_ids
            .iter()
            .any(|id| portfolio.member(*id).is_none())
        {
            return Err(AppError::invalid("Un des membres liés est introuvable"));
        }
    }
    Ok(goal)
}

pub struct GoalService {
    pub(crate) reader: PortfolioReader,
    pub(crate) goals: Arc<dyn GoalRepository>,
}

/// Avancement : montant courant = valeur détenue par le foyer dans les comptes liés (hors dettes).
fn view(goal: Goal, portfolio: &Portfolio) -> GoalView {
    let current = portfolio.household_value_of(&goal.account_ids);
    GoalView {
        progress: goal.progress(current, portfolio.today),
        goal,
    }
}

impl GoalService {
    pub async fn list(&self) -> AppResult<Vec<GoalView>> {
        let portfolio = self.reader.load().await?;
        Ok(self
            .goals
            .list()
            .await?
            .into_iter()
            .map(|goal| view(goal, &portfolio))
            .collect())
    }

    pub async fn create(&self, input: &GoalInput) -> AppResult<GoalView> {
        let portfolio = self.reader.load().await?;
        let goal = build_goal(None, input, &portfolio)?;
        self.goals.save(&goal).await?;
        Ok(view(goal, &portfolio))
    }

    pub async fn update(&self, id: Uuid, input: &GoalInput) -> AppResult<GoalView> {
        let portfolio = self.reader.load().await?;
        let id = GoalId::from_uuid(id);
        let existing = self
            .goals
            .list()
            .await?
            .into_iter()
            .find(|goal| goal.id == id);
        let existing = existing.ok_or(AppError::NotFound("Objectif introuvable"))?;
        let goal = build_goal(Some(&existing), input, &portfolio)?;
        self.goals.save(&goal).await?;
        Ok(view(goal, &portfolio))
    }

    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        if self.goals.delete(GoalId::from_uuid(id)).await? {
            Ok(())
        } else {
            Err(AppError::NotFound("Objectif introuvable"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{TestApp, date};
    use fineme_domain::goal::GoalStatus;

    fn safety_net(accounts: &[AccountId]) -> GoalInput {
        GoalInput {
            name: Some("Matelas".into()),
            kind: Some("safety_net".into()),
            monthly_expenses: Some(Some(2000.0)),
            coverage_months: Some(Some(6.0)),
            account_ids: Some(accounts.iter().map(|id| id.as_uuid()).collect()),
            ..Default::default()
        }
    }

    fn project(accounts: &[AccountId]) -> GoalInput {
        GoalInput {
            name: Some("Voyage".into()),
            kind: Some("project".into()),
            target_amount: Some(Some(20_000.0)),
            target_date: Some(Some(date(2028, 1, 1))),
            monthly_contribution: Some(Some(500.0)),
            expected_return_pct: Some(Some(5.0)),
            account_ids: Some(accounts.iter().map(|id| id.as_uuid()).collect()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn safety_net_underfunded_then_funded() {
        let t = TestApp::household().await;
        let savings = t.checking(&[(t.alice, 1.0)], 9000.0).await;
        let view = t.app.goals.create(&safety_net(&[savings])).await.unwrap();
        assert_eq!(view.progress.status, GoalStatus::Underfunded);
        assert_eq!(view.progress.target_amount, 12_000.0);
        assert_eq!(view.progress.coverage_months, Some(4.5));
        t.checking(&[(t.alice, 1.0)], 0.0).await;
        let more = t.checking(&[(t.bob, 1.0)], 3000.0).await;
        let patch = GoalInput {
            account_ids: Some(vec![savings.as_uuid(), more.as_uuid()]),
            ..Default::default()
        };
        let view = t
            .app
            .goals
            .update(view.goal.id.as_uuid(), &patch)
            .await
            .unwrap();
        assert_eq!(view.progress.status, GoalStatus::Funded);
    }

    #[tokio::test]
    async fn current_amount_is_household_owned_value_excluding_debts() {
        let t = TestApp::household().await;
        let joint = t.checking(&[(t.alice, 0.6), (t.bob, 0.2)], 10_000.0).await;
        let (_, loan) = t.house_with_loan().await;
        let view = t
            .app
            .goals
            .create(&project(&[joint, loan, joint]))
            .await
            .unwrap();
        assert!((view.progress.current_amount - 8000.0).abs() < 1e-9);
        assert_eq!(view.goal.account_ids, vec![joint, loan]);
    }

    #[tokio::test]
    async fn project_statuses_follow_the_clock() {
        let t = TestApp::household().await;
        let account = t.checking(&[(t.alice, 1.0)], 10_000.0).await;
        let id = t
            .app
            .goals
            .create(&project(&[account]))
            .await
            .unwrap()
            .goal
            .id
            .as_uuid();
        let status = |views: Vec<GoalView>| views[0].progress.status;
        assert_eq!(
            status(t.app.goals.list().await.unwrap()),
            GoalStatus::OnTrack
        );
        let hard = GoalInput {
            monthly_contribution: Some(Some(50.0)),
            expected_return_pct: Some(None),
            ..Default::default()
        };
        let view = t.app.goals.update(id, &hard).await.unwrap();
        assert_eq!(view.progress.status, GoalStatus::Challenging);
        assert!(view.progress.required_return_pct.unwrap() > 0.0);
        t.clock.advance_days(365 * 3);
        assert_eq!(
            status(t.app.goals.list().await.unwrap()),
            GoalStatus::Overdue
        );
        let done = GoalInput {
            target_amount: Some(Some(5000.0)),
            ..Default::default()
        };
        assert_eq!(
            t.app.goals.update(id, &done).await.unwrap().progress.status,
            GoalStatus::Completed
        );
    }

    #[tokio::test]
    async fn goal_without_accounts_has_zero_progress() {
        let t = TestApp::household().await;
        let view = t.app.goals.create(&project(&[])).await.unwrap();
        assert_eq!(view.progress.current_amount, 0.0);
        assert!(view.progress.current_amount.is_sign_positive());
        assert_eq!(view.progress.pct, 0.0);
    }

    #[tokio::test]
    async fn validation_of_kind_fields_and_links() {
        let t = TestApp::household().await;
        let no_kind = GoalInput {
            kind: None,
            ..project(&[])
        };
        assert!(matches!(
            t.app.goals.create(&no_kind).await,
            Err(AppError::Invalid(_))
        ));
        let bad_kind = GoalInput {
            kind: Some("dream".into()),
            ..project(&[])
        };
        assert!(matches!(
            t.app.goals.create(&bad_kind).await,
            Err(AppError::Domain(_))
        ));
        let no_target = GoalInput {
            target_amount: None,
            ..project(&[])
        };
        assert!(matches!(
            t.app.goals.create(&no_target).await,
            Err(AppError::Domain(_))
        ));
        let no_coverage = GoalInput {
            coverage_months: None,
            ..safety_net(&[])
        };
        assert!(matches!(
            t.app.goals.create(&no_coverage).await,
            Err(AppError::Domain(_))
        ));
        let unknown_account = project(&[AccountId::generate()]);
        assert!(matches!(
            t.app.goals.create(&unknown_account).await,
            Err(AppError::Invalid(_))
        ));
        let unknown_member = GoalInput {
            member_ids: Some(vec![Uuid::new_v4()]),
            ..project(&[])
        };
        assert!(matches!(
            t.app.goals.create(&unknown_member).await,
            Err(AppError::Invalid(_))
        ));
        assert!(t.app.goals.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn update_keeps_links_unless_given_and_delete_works() {
        let t = TestApp::household().await;
        let account = t.checking(&[(t.alice, 1.0)], 100.0).await;
        let input = GoalInput {
            member_ids: Some(vec![t.alice.as_uuid(), t.bob.as_uuid()]),
            ..project(&[account])
        };
        let id = t.app.goals.create(&input).await.unwrap().goal.id.as_uuid();
        let renamed = GoalInput {
            name: Some("Tour du monde".into()),
            icon: Some(Some("✈".into())),
            ..Default::default()
        };
        let goal = t.app.goals.update(id, &renamed).await.unwrap().goal;
        assert_eq!(
            (
                goal.name.as_str(),
                goal.account_ids.len(),
                goal.member_ids.len()
            ),
            ("Tour du monde", 1, 2)
        );
        assert!(matches!(
            t.app.goals.update(Uuid::new_v4(), &renamed).await,
            Err(AppError::NotFound(_))
        ));
        t.app.goals.delete(id).await.unwrap();
        assert!(matches!(
            t.app.goals.delete(id).await,
            Err(AppError::NotFound(_))
        ));
    }
}
