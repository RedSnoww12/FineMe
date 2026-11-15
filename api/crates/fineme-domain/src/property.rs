use chrono::NaiveDate;

use crate::error::DomainError;
use crate::ids::PropertyId;

macro_rules! string_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? } invalid = $message:literal) => {
        $(#[$doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn parse(value: &str) -> Result<Self, $crate::error::DomainError> {
                match value {
                    $($text => Ok($name::$variant),)+
                    _ => Err($crate::error::DomainError::Rule($message)),
                }
            }

            pub const fn as_str(&self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }
        }
    };
}
pub(crate) use string_enum;

string_enum!(PropertyCategory { Primary => "primary", Secondary => "secondary", Rental => "rental" }
    invalid = "Catégorie de bien invalide (primary, secondary, rental)");

string_enum!(BuildingType { House => "house", Apartment => "apartment", Land => "land", Parking => "parking", Other => "other" }
    invalid = "Type de bien invalide (house, apartment, land, parking, other)");

/// Bien immobilier porté par un compte de type `real_estate`. Montants dans la devise du compte.
#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub id: PropertyId,
    pub address: Option<String>,
    pub category: PropertyCategory,
    pub building_type: BuildingType,
    pub surface_m2: Option<f64>,
    pub purchase_price: f64,
    /// Frais de notaire / d'agence.
    pub purchase_fees: f64,
    pub purchased_at: Option<NaiveDate>,
    pub current_value: f64,
    /// 0 si non loué.
    pub monthly_rent: f64,
    pub monthly_charges: f64,
    pub yearly_property_tax: f64,
}

impl Property {
    /// Prix de revient : prix d'achat + frais.
    pub fn cost(&self) -> f64 {
        self.purchase_price + self.purchase_fees
    }

    /// Tous les montants doivent être finis et positifs ou nuls.
    pub fn check(&self) -> Result<(), DomainError> {
        use crate::error::non_negative;
        if let Some(surface) = self.surface_m2 {
            non_negative(surface, "surface_m2")?;
        }
        non_negative(self.purchase_price, "purchase_price")?;
        non_negative(self.purchase_fees, "purchase_fees")?;
        non_negative(self.current_value, "current_value")?;
        non_negative(self.monthly_rent, "monthly_rent")?;
        non_negative(self.monthly_charges, "monthly_charges")?;
        non_negative(self.yearly_property_tax, "yearly_property_tax")?;
        Ok(())
    }

    /// Rendements locatifs (brut, net) en % ; `None` si le bien n'est pas loué.
    /// Base : prix de revient, à défaut la valeur actuelle.
    pub fn rental_yields(&self) -> (Option<f64>, Option<f64>) {
        let base = if self.cost() > 0.0 {
            self.cost()
        } else {
            self.current_value
        };
        if self.monthly_rent <= 0.0 || base <= 0.0 {
            return (None, None);
        }
        let gross = self.monthly_rent * 12.0;
        let net = gross - self.monthly_charges * 12.0 - self.yearly_property_tax;
        (Some(gross / base * 100.0), Some(net / base * 100.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn property() -> Property {
        Property {
            id: PropertyId::generate(),
            address: None,
            category: PropertyCategory::Rental,
            building_type: BuildingType::Apartment,
            surface_m2: Some(40.0),
            purchase_price: 190_000.0,
            purchase_fees: 10_000.0,
            purchased_at: None,
            current_value: 250_000.0,
            monthly_rent: 1000.0,
            monthly_charges: 100.0,
            yearly_property_tax: 1200.0,
        }
    }

    #[test]
    fn yields_on_cost() {
        let (gross, net) = property().rental_yields();
        assert!((gross.unwrap() - 6.0).abs() < 1e-9);
        assert!((net.unwrap() - 4.8).abs() < 1e-9);
    }

    #[test]
    fn no_yield_without_rent() {
        assert_eq!(
            Property {
                monthly_rent: 0.0,
                ..property()
            }
            .rental_yields(),
            (None, None)
        );
    }

    #[test]
    fn yields_fall_back_to_current_value_without_cost() {
        let p = Property {
            purchase_price: 0.0,
            purchase_fees: 0.0,
            current_value: 120_000.0,
            ..property()
        };
        assert!((p.rental_yields().0.unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn check_rejects_negative_amounts() {
        assert!(property().check().is_ok());
        assert_eq!(
            Property {
                monthly_rent: -1.0,
                ..property()
            }
            .check(),
            Err(DomainError::Negative("monthly_rent"))
        );
        assert_eq!(
            Property {
                surface_m2: Some(f64::NAN),
                ..property()
            }
            .check(),
            Err(DomainError::NotFinite("surface_m2"))
        );
    }

    #[test]
    fn enums_parse_and_print() {
        assert_eq!(
            PropertyCategory::parse("rental").unwrap().as_str(),
            "rental"
        );
        assert!(PropertyCategory::parse("castle").is_err());
        assert_eq!(BuildingType::ALL.len(), 5);
        assert!(BuildingType::parse("boat").is_err());
    }
}
