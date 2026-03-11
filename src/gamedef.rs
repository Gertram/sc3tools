use crate::resource_provider::ResourceProvider;
use crate::text::EncodingMaps;
use itertools::Itertools;
use nom::{
    bytes::complete::is_not,
    character::complete::{char, line_ending, not_line_ending},
    combinator::{map, map_opt, map_res, opt},
    multi::separated_list0,
    sequence::{delimited, pair, preceded, tuple},
    Finish, IResult,
};
use serde::Deserialize;
use serde_json;
use std::{any::type_name, borrow::Cow, collections::HashMap, ops::RangeInclusive};

pub struct GameDef {
    #[allow(dead_code)]
    pub full_name: String,
    pub aliases: Vec<String>,
    #[allow(dead_code)]
    reserved_codepoints: Option<RangeInclusive<char>>,
    charset: Vec<char>,
    pub compound_chars: HashMap<char, String>,
    pub encoding_maps: EncodingMaps,
    pub fullwidth_blocklist: Vec<char>,
}

#[derive(Deserialize)]
pub struct GameDefJson<'a> {
    pub name: String,
    pub resource_dir: &'a str,
    pub aliases: Vec<String>,
    #[allow(dead_code)]
    pub reserved_codepoints: Option<RangeInclusive<char>>,
    pub fullwidth_blocklist: Vec<char>,
}

pub trait TryFromResource<T> {
    type Error;
    fn try_from_with<Provider: ResourceProvider>(value: T) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

impl<'a> TryFromResource<GameDefJson<'a>> for GameDef {
    type Error = String;
    fn try_from_with<Provider: ResourceProvider>(json: GameDefJson<'a>) -> Result<Self, Self::Error> {
        Self::new::<Provider>(
            json.name,
            json.resource_dir,
            json.aliases,
            json.reserved_codepoints,
            json.fullwidth_blocklist,
        )
    }
}

impl GameDef {
    pub fn new<Provider: ResourceProvider>(
        full_name: String,
        resource_dir: &str,
        aliases: Vec<String>,
        reserved_codepoints: Option<RangeInclusive<char>>,
        fullwidth_blocklist: Vec<char>,
    ) -> Result<Self, String> {
        fn file_path(resource_dir: &str, name: &'static str) -> String {
            format!("{}/{}", resource_dir, name)
        }

        let charset: Cow<str> = Provider::get_to_string(&file_path(resource_dir, "charset.utf8"))
            .map_err(|e| format!("Failed to get charset for {}: {}", full_name, e))?;
        let charset: Vec<char> = charset.chars().collect();
        let compound_chars: Cow<str> = Provider::get_to_string(&file_path(resource_dir, "compound_chars.map"))
            .map_err(|e| format!("Failed to get compound_chars for {}: {}", full_name, e))?;
        let compound_chars = parse_compound_ch_map(&compound_chars)
            .map_err(|e| format!("Failed to parse compound_chars for {}: {}", full_name, e))?;
        let encoding_maps = EncodingMaps::new(&charset, &compound_chars)
            .map_err(|err| {
                format!(
                    "Error while constructing encoding maps for {}. \
                    The following Private Use Area characters were not found in the charset: [{}]",
                    full_name,
                    err.missing_pua_chars
                        .into_iter()
                        .map(|ch| format!("'{}'", ch.escape_unicode()))
                        .join(", ")
                )
            })?;

        let def = Self {
            full_name,
            aliases,
            reserved_codepoints,
            charset,
            compound_chars,
            encoding_maps,
            fullwidth_blocklist,
        };
        Ok(def)
    }

    pub fn charset(&self) -> &[char] {
        &self.charset
    }

}

pub fn get_by_alias<'a>(defs: &'a [GameDef], alias: &str) -> Option<&'a GameDef> {
    defs.iter().find(|x| x.aliases.iter().any(|a| a == alias))
}

pub fn build_gamedefs_from_json<Provider: ResourceProvider>(json: &str) -> Result<Vec<GameDef>, String> {
    let defs: Vec<GameDefJson> = serde_json::from_str(json)
        .map_err(|e| format!("Failed parse gamedef from {}: {}", type_name::<Provider>(), e))?;
    let defs = defs
        .into_iter()
        .map(GameDef::try_from_with::<Provider>)
        .collect::<Result<Vec<GameDef>, String>>()?;
    Ok(defs)
}

#[derive(Eq, PartialEq, Debug)]
struct PuaMapping<'a> {
    codepoint_range: RangeInclusive<char>,
    ch: &'a str,
}

impl<'a> PuaMapping<'a> {
    fn new(codepoint_range: RangeInclusive<char>, ch: &'a str) -> Self {
        Self {
            codepoint_range,
            ch,
        }
    }

    pub fn parse(i: &str) -> IResult<&str, PuaMapping> {
        fn codepoint(i: &str) -> IResult<&str, char> {
            map_opt(
                map_res(is_not("-]"), |s| u32::from_str_radix(s, 16)),
                std::char::from_u32,
            )(i)
        }

        fn range(i: &str) -> IResult<&str, RangeInclusive<char>> {
            map(
                delimited(
                    char('['),
                    pair(codepoint, opt(preceded(char('-'), codepoint))),
                    char(']'),
                ),
                |(a, b)| match (a, b) {
                    (a, Some(b)) => (a..=b),
                    _ => (a..=a),
                },
            )(i)
        }

        map(tuple((range, char('='), not_line_ending)), |(r, _, ch)| {
            PuaMapping::new(r, ch)
        })(i)
    }
}

fn parse_compound_ch_map(i: &str) -> Result<HashMap<char, String>, String> {
    let (remaining, mappings) = separated_list0(line_ending, PuaMapping::parse)(i)
        .finish()
        .map_err(|e| format!("Parsing error: {:?}", e))?;

    let tail = remaining.trim();
    if !tail.is_empty() {
        let preview = if tail.len() > 50 { format!("{:.50}...", tail) } else { tail.to_owned() };
        return Err(format!("Parsing stopped early. Unhandled data: {:?}", preview));
    }

    let mappings = mappings
        .iter()
        .flat_map(|m| {
            m.codepoint_range
                .clone()
                .map(move |codepoint| (codepoint, m.ch.to_string()))
        })
        .collect();
    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pua_mapping() {
        assert_eq!(
            PuaMapping::parse("[E01C]=meow").unwrap().1,
            PuaMapping::new('\u{E01C}'..='\u{E01C}', "meow")
        );

        assert_eq!(
            PuaMapping::parse("[E01C-E01F]=¹⁸").unwrap().1,
            PuaMapping::new('\u{E01C}'..='\u{E01F}', "¹⁸")
        );
    }
}
