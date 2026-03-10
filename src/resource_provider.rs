use rust_embed::RustEmbed;
use std::{borrow::Cow, fs, path::PathBuf};

pub const RESOURCES_DIR: &str = "resources/";

#[derive(RustEmbed)]
#[folder = "resources/"]
pub struct ResourceDir;

pub trait ResourceProvider {
    fn get(path: &str) -> Cow<'static, [u8]>;
    fn get_to_string(path: &str) -> Cow<'static, str> {
        match Self::get(path) {
            Cow::Borrowed(bytes) => std::str::from_utf8(bytes).map(Cow::Borrowed).unwrap(),
            Cow::Owned(bytes) => String::from_utf8(bytes).map(Cow::Owned).unwrap(),
        }
    }
}

pub struct EmbedResourceProvider;
impl ResourceProvider for EmbedResourceProvider {
    fn get(path: &str) -> Cow<'static, [u8]> {
        ResourceDir::get(path).unwrap()
    }
}

pub struct FsResourceProvider;
impl FsResourceProvider {
    fn full_path(path: &str) -> PathBuf {
        PathBuf::from(RESOURCES_DIR).join(path)
    }
    pub fn exists(path: &str) -> bool {
        Self::full_path(path).exists()
    }
}
impl ResourceProvider for FsResourceProvider {
    fn get(path: &str) -> Cow<'static, [u8]> {
        fs::read(Self::full_path(path)).map(Cow::Owned).unwrap()
    }
    fn get_to_string(path: &str) -> Cow<'static, str> {
        fs::read_to_string(Self::full_path(path))
            .map(Cow::Owned)
            .unwrap()
    }
}
