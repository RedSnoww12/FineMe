-- Nettoyage de l'historique : un point corrigé à la main garde ses totaux saisis et perd son détail par compte.
ALTER TABLE snapshots ADD COLUMN corrected BOOLEAN NOT NULL DEFAULT false;
