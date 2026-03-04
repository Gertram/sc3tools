use crate::util::IndentError;
use rust_embed::RustEmbed;
use std::{borrow::Cow, fmt::Display, fs, path::PathBuf};

pub const RESOURCES_DIR: &str = "resources/";

#[derive(RustEmbed)]
#[folder = "resources/"]
pub struct ResourceDir;

fn utf8_err<E: Display>(path: &str, e: E) -> String {
    format!("Invalid UTF-8 in {}:\n{}", path, e.indent())
}

pub trait ResourceProvider {
    fn get(path: &str) -> Result<Cow<'static, [u8]>, String>;
    fn get_to_string(path: &str) -> Result<Cow<'static, str>, String> {
        match Self::get(path)? {
            Cow::Borrowed(b) => std::str::from_utf8(b)
                .map(Cow::Borrowed)
                .map_err(|e| utf8_err(path, e)),
            Cow::Owned(v) => String::from_utf8(v)
                .map(Cow::Owned)
                .map_err(|e| utf8_err(path, e)),
        }
    }
}

pub struct EmbedResourceProvider;
impl ResourceProvider for EmbedResourceProvider {
    fn get(path: &str) -> Result<Cow<'static, [u8]>, String> {
        ResourceDir::get(path)
            .map(|file| file)
            .ok_or_else(|| format!("Could not read embed resource {}", path))
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
    fn read_err<E: Display>(path: &str, e: E) -> String {
        format!("Could not read fs resource {}:\n{}", path, e.indent())
    }
}
impl ResourceProvider for FsResourceProvider {
    fn get(path: &str) -> Result<Cow<'static, [u8]>, String> {
        fs::read(Self::full_path(path))
            .map(Cow::Owned)
            .map_err(|e| Self::read_err(path, e))
    }
    fn get_to_string(path: &str) -> Result<Cow<'static, str>, String> {
        fs::read_to_string(Self::full_path(path))
            .map(Cow::Owned)
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::InvalidData {
                    return utf8_err(path, e);
                } else {
                    Self::read_err(path, e)
                }
            })
    }
}
