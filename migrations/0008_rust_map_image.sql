-- The picture a Rust server uploads of its map, kept in case a start from a save does not link it again.
ALTER TABLE servers ADD COLUMN map_image TEXT;
