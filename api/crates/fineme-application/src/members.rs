use std::sync::Arc;

use fineme_domain::ids::MemberId;
use fineme_domain::member::{Member, default_color};
use uuid::Uuid;

use crate::commands::clean;
use crate::error::{AppError, AppResult};
use crate::ports::{PortfolioRepository, RepoError};

pub struct MemberService {
    pub(crate) repo: Arc<dyn PortfolioRepository>,
}

fn valid_name(name: &str) -> AppResult<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::invalid("Le nom du membre est requis"));
    }
    Ok(name.to_string())
}

fn name_conflict(error: RepoError) -> AppError {
    match error {
        RepoError::Conflict => AppError::Conflict("Un membre porte déjà ce nom"),
        other => other.into(),
    }
}

impl MemberService {
    pub async fn list(&self) -> AppResult<Vec<Member>> {
        Ok(self.repo.members().await?)
    }

    pub async fn create(&self, name: Option<String>, color: Option<String>) -> AppResult<Member> {
        let name = valid_name(name.as_deref().unwrap_or(""))?;
        let existing = self.repo.members().await?;
        let member = Member {
            id: MemberId::generate(),
            name,
            color: clean(color).unwrap_or_else(|| default_color(existing.len()).to_string()),
            is_primary: !existing.iter().any(|member| member.is_primary),
        };
        self.repo
            .insert_member(&member)
            .await
            .map_err(name_conflict)?;
        Ok(member)
    }

    pub async fn update(
        &self,
        id: Uuid,
        name: Option<String>,
        color: Option<String>,
    ) -> AppResult<Member> {
        let mut member = self.find(id).await?;
        if let Some(name) = name {
            member.name = valid_name(&name)?;
        }
        if let Some(color) = clean(color) {
            member.color = color;
        }
        self.repo
            .update_member(&member)
            .await
            .map_err(name_conflict)?;
        Ok(member)
    }

    /// Refusé pour le membre principal ou un membre encore détenteur d'un compte.
    pub async fn delete(&self, id: Uuid) -> AppResult<()> {
        let member = self.find(id).await?;
        if member.is_primary {
            return Err(AppError::Conflict(
                "Le membre principal ne peut pas être supprimé",
            ));
        }
        let data = self.repo.load().await?;
        if data
            .accounts
            .iter()
            .any(|account| account.owners.includes(member.id))
        {
            return Err(AppError::Conflict(
                "Ce membre est encore propriétaire d'au moins un compte",
            ));
        }
        Ok(self.repo.delete_member(member.id).await?)
    }

    async fn find(&self, id: Uuid) -> AppResult<Member> {
        let id = MemberId::from_uuid(id);
        self.repo
            .members()
            .await?
            .into_iter()
            .find(|member| member.id == id)
            .ok_or(AppError::NotFound("Membre introuvable"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestApp;
    use fineme_domain::member::MEMBER_PALETTE;

    #[tokio::test]
    async fn create_assigns_palette_color_and_trims_name() {
        let t = TestApp::household().await;
        let third = t
            .app
            .members
            .create(Some("  Charlie ".into()), None)
            .await
            .unwrap();
        assert_eq!(third.name, "Charlie");
        assert_eq!(third.color, MEMBER_PALETTE[2]);
        assert!(!third.is_primary);
        let custom = t
            .app
            .members
            .create(Some("Dana".into()), Some("#123456".into()))
            .await
            .unwrap();
        assert_eq!(custom.color, "#123456");
    }

    #[tokio::test]
    async fn first_member_ever_becomes_primary() {
        let t = TestApp::empty().await;
        assert!(
            t.app
                .members
                .create(Some("Seul".into()), None)
                .await
                .unwrap()
                .is_primary
        );
        assert!(
            !t.app
                .members
                .create(Some("Second".into()), None)
                .await
                .unwrap()
                .is_primary
        );
    }

    #[tokio::test]
    async fn name_is_required_and_unique() {
        let t = TestApp::household().await;
        assert!(matches!(
            t.app.members.create(Some("  ".into()), None).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app.members.create(None, None).await,
            Err(AppError::Invalid(_))
        ));
        assert!(matches!(
            t.app.members.create(Some("Bob".into()), None).await,
            Err(AppError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn update_changes_only_given_fields() {
        let t = TestApp::household().await;
        let updated = t
            .app
            .members
            .update(t.bob.as_uuid(), None, Some("#00ff00".into()))
            .await
            .unwrap();
        assert_eq!(
            (updated.name.as_str(), updated.color.as_str()),
            ("Bob", "#00ff00")
        );
        let renamed = t
            .app
            .members
            .update(t.bob.as_uuid(), Some("Robert".into()), None)
            .await
            .unwrap();
        assert_eq!(
            (renamed.name.as_str(), renamed.color.as_str()),
            ("Robert", "#00ff00")
        );
        assert!(matches!(
            t.app.members.update(Uuid::new_v4(), None, None).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            t.app
                .members
                .update(t.bob.as_uuid(), Some("Alice".into()), None)
                .await,
            Err(AppError::Conflict(_))
        ));
    }

    #[tokio::test]
    async fn delete_is_refused_for_primary_and_for_owners() {
        let t = TestApp::household().await;
        t.checking(&[(t.alice, 0.6), (t.bob, 0.2)], 1000.0).await;
        assert!(matches!(
            t.app.members.delete(t.alice.as_uuid()).await,
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            t.app.members.delete(t.bob.as_uuid()).await,
            Err(AppError::Conflict(_))
        ));
        assert!(matches!(
            t.app.members.delete(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
    }

    #[tokio::test]
    async fn delete_member_without_accounts() {
        let t = TestApp::household().await;
        t.app.members.delete(t.bob.as_uuid()).await.unwrap();
        assert_eq!(t.app.members.list().await.unwrap().len(), 1);
    }
}
