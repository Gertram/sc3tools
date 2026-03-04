use crate::gamedef::{GameDef, StrictParse};
use crate::{gamedef, resource_provider};
use std::fmt::Display;

pub fn build_embed_gamedefs_from_json(json: &str) -> Result<Vec<GameDef>, String> {
    gamedef::build_gamedefs_from_json::<resource_provider::EmbedResourceProvider, StrictParse>(json)
}

pub trait IndentError {
    fn indent(self) -> String;
}

impl<T: Display> IndentError for T {
    fn indent(self) -> String {
        indent(self)
    }
}

pub fn indent<T: Display>(err: T) -> String {
    let s = err.to_string();
    s.lines()
        .map(|line| format!("\t{}", line))
        .collect::<Vec<_>>()
        .join("\n")
}
