-- Historique des cours récupérés automatiquement : un point par position et par jour (le dernier du jour
-- l'emporte), dans la devise de la position. Supprimé avec la position.
CREATE TABLE price_history (
    holding_id UUID NOT NULL REFERENCES holdings(id) ON DELETE CASCADE,
    date DATE NOT NULL,
    price DOUBLE PRECISION NOT NULL,
    currency TEXT NOT NULL,
    PRIMARY KEY (holding_id, date)
);
