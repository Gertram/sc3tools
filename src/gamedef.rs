use crate::resource_provider::ResourceProvider;
use crate::text::EncodingMaps;
use itertools::Itertools;
use nom::{
    branch::alt,
    character::complete::{char, hex_digit1, line_ending, not_line_ending},
    combinator::{cut, map, map_opt, map_res, opt, verify},
    multi::separated_list0,
    sequence::{delimited, pair, preceded, tuple},
    Finish, IResult,
};
use nom_locate::LocatedSpan;
use serde::Deserialize;
use serde_json;
use std::{any::type_name, borrow::Cow, collections::HashMap, ops::RangeInclusive, sync::Arc};

pub struct GameDef {
    #[allow(dead_code)]
    pub full_name: String,
    pub aliases: Vec<String>,
    #[allow(dead_code)]
    reserved_codepoints: Option<RangeInclusive<char>>,
    charset: Vec<char>,
    pub compound_chars: HashMap<char, Arc<str>>,
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
    fn try_from_with<Provider: ResourceProvider, Policy: ParsePolicy>(value: T) -> Result<Self, Self::Error>
    where
        Self: Sized;
}

impl<'a> TryFromResource<GameDefJson<'a>> for GameDef {
    type Error = String;
    fn try_from_with<Provider: ResourceProvider, Policy: ParsePolicy>(json: GameDefJson<'a>) -> Result<Self, Self::Error> {
        Self::new::<Provider, Policy>(
            json.name,
            json.resource_dir,
            json.aliases,
            json.reserved_codepoints,
            json.fullwidth_blocklist,
        )
    }
}

impl GameDef {
    pub fn new<Provider: ResourceProvider, Policy: ParsePolicy>(
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
        let compound_chars = parse_compound_ch_map::<Policy>(&compound_chars)
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

pub fn build_gamedefs_from_json<Provider: ResourceProvider, Policy: ParsePolicy>(json: &str) -> Result<Vec<GameDef>, String> {
    let defs: Vec<GameDefJson> = serde_json::from_str(json)
        .map_err(|e| format!("Failed parse gamedef from {}: {}", type_name::<Provider>(), e))?;
    let defs = defs
        .into_iter()
        .filter_map(|d| {
            let config_name = d.name.clone();

            GameDef::try_from_with::<Provider, Policy>(d)
                .map(Some)
                .or_else(|e| Policy::handle_element_error(&config_name, &e).map(|_| None))
                .transpose()
        })
        .collect::<Result<Vec<GameDef>, String>>()?;
    Ok(defs)
}

pub trait ParsePolicy {
    fn on_invalid_line(line: u32, col: usize, text: &str) -> Result<(), ()>;
    fn on_unparsed_tail(line: u32, col: usize, tail: &str) -> Result<(), String>;
    fn handle_element_error(config_name: &str, err: &str) -> Result<(), String>;
}

pub struct StrictParse;
impl ParsePolicy for StrictParse {
    fn on_invalid_line(_line: u32, _col: usize, _text: &str) -> Result<(), ()> {
        Err(())
    }
    fn on_unparsed_tail(line: u32, col: usize, tail: &str) -> Result<(), String> {
        Err(format!("Parsing stopped early at line {}, col {}. Unhandled tail: {:.50}", line, col, tail))
    }
    fn handle_element_error(config_name: &str, err: &str) -> Result<(), String> {
        Err(format!("Critical error in '{}': {}", config_name, err))
    }
}

pub struct LenientParse;
impl ParsePolicy for LenientParse {
    fn on_invalid_line(line: u32, col: usize, text: &str) -> Result<(), ()> {
        eprintln!("WARNING: Skipping invalid data (line {}, col {}): {}", line, col, text);
        Ok(())
    }
    fn on_unparsed_tail(line: u32, col: usize, tail: &str) -> Result<(), String> {
        eprintln!("WARNING: Unparsed tail at line {}, col {}. Unhandled tail: {:.50}", line, col, tail);
        Ok(())
    }
    fn handle_element_error(config_name: &str, err: &str) -> Result<(), String> {
        eprintln!("Warning: Skipping game '{}' due to error: {}", config_name, err);
        Ok(())
    }
}

#[derive(Eq, PartialEq, Debug)]
struct PuaMapping<'a> {
    codepoint_range: RangeInclusive<char>,
    ch: &'a str,
}

type Span<'a> = LocatedSpan<&'a str>;

impl<'a> PuaMapping<'a> {
    fn new(codepoint_range: RangeInclusive<char>, ch: &'a str) -> Self {
        Self {
            codepoint_range,
            ch,
        }
    }

    pub fn parse(i: Span) -> IResult<Span, PuaMapping> {
        fn codepoint(i: Span) -> IResult<Span, char> {
            map_opt(
                map_res(hex_digit1, |s: Span| u32::from_str_radix(s.fragment(), 16)),
                std::char::from_u32,
            )(i)
        }

        fn range(i: Span) -> IResult<Span, RangeInclusive<char>> {
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
            PuaMapping::new(r, ch.fragment())
        })(i)
    }
}

fn parse_line<P: ParsePolicy>(i: Span) -> IResult<Span, Option<PuaMapping>> {
    alt((
        map(PuaMapping::parse, Some),
        map(preceded(char('#'), not_line_ending), |_| None),
        map(verify(not_line_ending, |s: &Span| s.fragment().trim().is_empty()), |_| None),
        cut(map_res(not_line_ending, |bad: Span| {
            P::on_invalid_line(bad.location_line(), bad.get_utf8_column(), bad.fragment())
                .map(|_| None)
        })),
    ))(i)
}

fn parse_compound_ch_map<Policy: ParsePolicy>(i: &str) -> Result<HashMap<char, Arc<str>>, String> {
    let input_span = Span::new(i);

    let (remaining, mappings) = separated_list0(line_ending, parse_line::<Policy>)(input_span)
        .finish()
        .map_err(|e| {
            let fragment = e.input.fragment();

            let limit = fragment.len().min(256);
            let peek = &fragment[..limit];

            let end = peek.find('\n').unwrap_or(limit);
            let error_line_text = &peek[..end].trim_end_matches('\r');
            format!(
                "Parsing error at line {}, column {}: {}",
                e.input.location_line(),
                e.input.get_utf8_column(),
                error_line_text
            )
        })?;

    let tail = remaining.fragment().trim();
    if !tail.is_empty() {
        Policy::on_unparsed_tail(remaining.location_line(), remaining.get_utf8_column(), tail)?;
    }

    let mappings = mappings
        .into_iter()
        .flatten()
        .flat_map(|PuaMapping { codepoint_range, ch }|{
            let shared: Arc<str> = Arc::from(ch);
            codepoint_range
                .map(move |codepoint| (codepoint, Arc::clone(&shared)))
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
            PuaMapping::parse(Span::new("[E01C]=meow")).unwrap().1,
            PuaMapping::new('\u{E01C}'..='\u{E01C}', "meow")
        );

        assert_eq!(
            PuaMapping::parse(Span::new("[E01C-E01F]=¹⁸")).unwrap().1,
            PuaMapping::new('\u{E01C}'..='\u{E01F}', "¹⁸")
        );
    }
}
