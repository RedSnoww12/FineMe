//! Préférences d'import CSV : la correspondance des colonnes retenue pour un compte est mémorisée,
//! pour que l'import suivant du même établissement ne demande aucun réglage.

use std::collections::BTreeMap;
use std::sync::Arc;

use fineme_domain::ids::AccountId;
use fineme_domain::import_profile::{ImportKind, ImportProfile};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::ports::{Clock, ImportProfileRepository};
use crate::reader::PortfolioReader;

pub struct ImportProfileService {
    pub(crate) reader: PortfolioReader,
    pub(crate) profiles: Arc<dyn ImportProfileRepository>,
    pub(crate) clock: Arc<dyn Clock>,
}

impl ImportProfileService {
    async fn existing_account(&self, account_id: Uuid) -> AppResult<AccountId> {
        let portfolio = self.reader.load().await?;
        portfolio
            .account(AccountId::from_uuid(account_id))
            .map(|account| account.id)
            .ok_or(AppError::NotFound("Compte introuvable"))
    }

    /// Préférences du compte, une par type de contenu au plus.
    pub async fn list(&self, account_id: Uuid) -> AppResult<Vec<ImportProfile>> {
        let account = self.existing_account(account_id).await?;
        let mut profiles = self.profiles.profiles(account).await?;
        profiles.sort_by_key(|profile| profile.kind().as_str());
        Ok(profiles)
    }

    /// Remplace la préférence du compte pour ce type.
    pub async fn save(
        &self,
        account_id: Uuid,
        kind: &str,
        template: Option<String>,
        columns: BTreeMap<String, String>,
    ) -> AppResult<ImportProfile> {
        let kind = ImportKind::parse(kind.trim())?;
        let account = self.existing_account(account_id).await?;
        let profile = ImportProfile::new(account, kind, template, columns, self.clock.now())?;
        self.profiles.save_profile(&profile).await?;
        Ok(profile)
    }
}

#[cfg(test)]
mod tests {
    use fineme_domain::DomainError;

    use super::*;
    use crate::testing::TestApp;

    fn columns(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(field, header)| (field.to_string(), header.to_string()))
            .collect()
    }

    #[tokio::test]
    async fn saved_mapping_is_replaced_per_kind_and_listed() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await.as_uuid();
        let profiles = &t.app.import_profiles;
        profiles
            .save(pea, "operations", None, columns(&[("date", "Date")]))
            .await
            .unwrap();
        let saved = profiles
            .save(
                pea,
                "operations",
                Some("courtier-generique".into()),
                columns(&[("date", "Date d'exécution"), ("kind", "Sens")]),
            )
            .await
            .unwrap();
        assert_eq!(saved.updated_at(), t.clock.now());
        profiles
            .save(pea, "holdings", None, columns(&[("name", "Valeur")]))
            .await
            .unwrap();
        let list = profiles.list(pea).await.unwrap();
        let kinds: Vec<&str> = list.iter().map(|p| p.kind().as_str()).collect();
        assert_eq!(kinds, vec!["holdings", "operations"]);
        assert_eq!(list[1].columns()["date"], "Date d'exécution");
        assert_eq!(list[1].template(), Some("courtier-generique"));
        // Un autre compte n'a pas de préférence.
        let other = t.investment(t.bob, 0.0).await.as_uuid();
        assert!(profiles.list(other).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_account_kind_or_field_is_refused() {
        let t = TestApp::household().await;
        let pea = t.investment(t.alice, 0.0).await.as_uuid();
        let profiles = &t.app.import_profiles;
        assert!(matches!(
            profiles.list(Uuid::new_v4()).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            profiles
                .save(
                    Uuid::new_v4(),
                    "holdings",
                    None,
                    columns(&[("name", "Nom")])
                )
                .await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            profiles
                .save(pea, "budget", None, columns(&[("name", "Nom")]))
                .await,
            Err(AppError::Domain(DomainError::Rule(_)))
        ));
        assert!(matches!(
            profiles
                .save(pea, "holdings", None, columns(&[("label", "Nom")]))
                .await,
            Err(AppError::Domain(_))
        ));
    }
}
