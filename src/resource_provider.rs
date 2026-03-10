use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "resources/"]
pub struct ResourceDir;

pub trait ResourceProvider {
    fn get(path: &str) -> Cow<'static, [u8]>;
}

pub struct EmbedResourceProvider;
impl ResourceProvider for EmbedResourceProvider {
    fn get(path: &str) -> Cow<'static, [u8]> {
        ResourceDir::get(path).unwrap()
    }
}
