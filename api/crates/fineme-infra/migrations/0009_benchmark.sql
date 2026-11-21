-- Indice de référence choisi par l'utilisateur (une seule ligne) et ses cours relevés chaque jour,
-- dans la devise de l'indice. Supprimer l'indice supprime ses cours (fait par l'application).
CREATE TABLE benchmark (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    name TEXT NOT NULL,
    symbol TEXT,
    isin TEXT,
    kind TEXT NOT NULL,
    currency TEXT NOT NULL,
    CHECK (symbol IS NOT NULL OR isin IS NOT NULL)
);

CREATE TABLE benchmark_prices (
    date DATE PRIMARY KEY,
    price DOUBLE PRECISION NOT NULL CHECK (price > 0)
);
