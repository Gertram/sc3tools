use crate::gamedef::GameDef;
use crate::{gamedef, resource_provider};

pub fn build_embed_gamedefs_from_json(json: &str) -> Result<Vec<GameDef>, String> {
    gamedef::build_gamedefs_from_json::<resource_provider::EmbedResourceProvider>(json)
}
