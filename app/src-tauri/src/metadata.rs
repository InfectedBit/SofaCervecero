//! Proveedores de metadatos y arte: IGDB, SteamGridDB, SteamStore. Ver plan §5.3 (M4). Stub M0.
#![allow(dead_code)]

/// Contrato de un proveedor de metadatos (enriquece juegos con datos/covers).
pub trait MetadataProvider {
    fn name(&self) -> &'static str;
    // fn enrich(&self, game_id: crate::library::GameId);
}
