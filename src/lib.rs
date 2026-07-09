use std::{collections::HashMap, path::Path};

use extism_pdk::*;
use nd_pdk::{
    host::library::get_library,
    lyrics::{Error, GetLyricsResponse, Lyrics, LyricsText},
};
use regex::Regex;

use crate::models::SyllableLyricsResponse;
mod models;
mod tags;

const BETA_URL: &str = "https://beta.music.apple.com";
const API_BASE_URL: &str = "https://amp-api.music.apple.com";

const MEDIA_TOKEN: &str = "media_token";
const USER_AGENT: &str = "user_agent";
const ACCOUNT_COUNTRY: &str = "storefront";
const CACHE_DAYS: &str = "cache_days";
const SKIP_CACHE: &str = "skip_cache";
const TRANSLATION_LANGUAGE: &str = "translation_language";

nd_pdk::register_lyrics!(APMLyricsFetcher);

#[derive(Default)]
struct APMLyricsFetcher;

impl Lyrics for APMLyricsFetcher {
    fn get_lyrics(
        &self,
        req: nd_pdk::lyrics::GetLyricsRequest,
    ) -> Result<nd_pdk::lyrics::GetLyricsResponse, Error> {
        let config = LoadedConfig::read()?;

        let file_path = if req.track.library_id > 0 && !req.track.path.is_empty() {
            let library = get_library(req.track.library_id)
                .map_err(|e| anyhow_string("get_library", e))?
                .ok_or(Error::new(format!(
                    "Unable to find the provided library with ID: {}",
                    req.track.library_id
                )))?;

            Path::new(&library.mount_point).join(&req.track.path)
        } else {
            return Err(Error::new("No permissions to get library data"));
        };

        info!("Probing {:?} for APM Catalog ID", &file_path);
        let apm_tags = tags::find_catalog_id(file_path)?.ok_or(Error::new(format!(
            "No tags found for song ID: {}",
            req.track.id
        )))?;
        let apm_catalog = coerce_to_digit(apm_tags)?;
        info!("Found APM Catalog ID: {}", apm_catalog);

        let jwt_cached = nd_pdk::host::cache::get_string("apm_jwt_token")
            .map_err(|err| anyhow_string("cache[apm_jwt_token]", err))?;

        let real_jwt = match jwt_cached {
            Some(jwt) => jwt,
            None => {
                let fetched_jwt = fetch_current_active_jwt(&config)
                    .map_err(|err| anyhow_string("fetch_current_active_jwt", err))?;

                // cache for 1 days (in seconds)
                nd_pdk::host::cache::set_string("apm_jwt_token", &fetched_jwt, 86400)
                    .map_err(|err| anyhow_string("cache[apm_jwt_token]", err))?;

                fetched_jwt
            }
        };

        debug!("APM Current JWT are: {}", &real_jwt);
        let lyrics_data = fetch_lyrics_for_catalog_id_with_cache(apm_catalog, &real_jwt, &config)
            .map_err(|err| anyhow_string("fetch_lyrics_for_catalog_id", err))?;

        Ok(GetLyricsResponse {
            lyrics: lyrics_data,
        })
    }
}

#[derive(Debug, Clone)]
struct LoadedConfig {
    media_token: String,
    user_agent: String,
    account_country: String,
    cache_days: i64,
    skip_cache: bool,
    translation_language: String,
}

impl LoadedConfig {
    fn read() -> Result<Self, Error> {
        let media_token = nd_pdk::host::config::get(MEDIA_TOKEN)
            .map_err(|e| anyhow_string("get_config[media_token]", e))?
            .ok_or(Error::new("Configuration not set for media_token"))?;

        let user_agent = nd_pdk::host::config::get(USER_AGENT)
            .map_err(|e| anyhow_string("get_config[user_agent]", e))?
            .ok_or(Error::new("Configuration not set for user_agent"))?;

        let account_country = nd_pdk::host::config::get(ACCOUNT_COUNTRY)
            .map_err(|e| anyhow_string("get_config[country]", e))?
            .ok_or(Error::new("Configuration not set for country"))?;

        let cache_days = nd_pdk::host::config::get_int(CACHE_DAYS)
            .map_err(|e| anyhow_string("get_config[cache_days]", e))?
            .ok_or(Error::new("Configuration not set for cache_days"))?;

        if cache_days < 1 || cache_days > 30 {
            return Err(Error::new("cache_days must be between 1 and 30"));
        }

        let skip_cache = nd_pdk::host::config::get(SKIP_CACHE)
            .map_err(|e| anyhow_string("get_config[skip_cache]", e))?
            .and_then(|v| Some(v.to_lowercase() == "true"))
            .unwrap_or(false);

        let translation_language = nd_pdk::host::config::get(TRANSLATION_LANGUAGE)
            .map_err(|e| anyhow_string("get_config[translation_language]", e))?
            .unwrap_or("en-GB".to_string());

        Ok(Self {
            media_token,
            user_agent,
            account_country,
            cache_days,
            skip_cache,
            translation_language,
        })
    }
}

fn anyhow_string(when: impl Into<String>, error: nd_pdk::host::Error) -> nd_pdk::lyrics::Error {
    let formatted_err = format!("{} failed: {:?}", when.into(), error);
    Error::new(formatted_err)
}

fn coerce_to_digit(cnid: String) -> Result<u64, Error> {
    cnid.parse::<u64>()
        .map_err(|e| Error::new(format!("Failed to parse content ID into number: {}", e)))
}

fn fetch_text(
    url: String,
    headers: HashMap<String, String>,
) -> Result<String, nd_pdk::host::Error> {
    let req = nd_pdk::host::http::HTTPRequest {
        method: "GET".to_string(),
        url,
        headers,
        no_follow_redirects: false,
        body: Vec::new(),
        timeout_ms: 10_000,
    };

    let resp = nd_pdk::host::http::send(req)?
        .ok_or_else(|| nd_pdk::host::Error::msg("No HTTP response received"))?;

    String::from_utf8(resp.body)
        .map_err(|e| nd_pdk::host::Error::msg(format!("Response body was not valid UTF-8: {}", e)))
}

fn resolve_asset_url(script_src: &str) -> String {
    if script_src.starts_with("http://") || script_src.starts_with("https://") {
        script_src.to_string()
    } else {
        format!("{}/{}", BETA_URL, script_src.trim_start_matches('/'))
    }
}

fn find_script_urls(html: &str) -> Vec<String> {
    let re = Regex::new(r#"<script[^>]+src="([^"]+\.js)""#).expect("valid regex");
    re.captures_iter(html)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

// Apple's web player signs its MusicKit JWTs with a fixed ES256 header
// ("kid":"WebPlayKid"), so we can look for that constant instead of trying
// to reverse-engineer which variable in the bundle holds the token.
fn find_jwt_token(js: &str) -> Option<String> {
    let re = Regex::new(
        r"eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6IldlYlBsYXlLaWQifQ\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
    )
    .expect("valid regex");
    re.find(js).map(|m| m.as_str().to_string())
}

fn fetch_current_active_jwt(config: &LoadedConfig) -> Result<String, nd_pdk::host::Error> {
    let mut basic_headers = HashMap::<String, String>::new();
    basic_headers.insert("User-Agent".to_string(), config.user_agent.to_string());
    basic_headers.insert("Origin".to_string(), BETA_URL.to_string());
    basic_headers.insert("Referer".to_string(), format!("{}/", BETA_URL));

    let home_html = fetch_text(
        format!("{}/{}/home", BETA_URL, config.account_country),
        basic_headers.clone(),
    )?;

    let script_urls = find_script_urls(&home_html);
    if script_urls.is_empty() {
        return Err(nd_pdk::host::Error::msg(
            "Failed to find any script assets on the Apple Music home page",
        ));
    }

    // The main entry bundle (non-legacy "index" chunk) is where the JWT
    // constant lives; try it first, then fall back to scanning the rest.
    let (mut ordered_urls, others): (Vec<String>, Vec<String>) = script_urls
        .into_iter()
        .partition(|url| url.contains("index") && url.contains(".js") && !url.contains("legacy"));
    ordered_urls.extend(others);

    for script_url in ordered_urls {
        let js_body = fetch_text(resolve_asset_url(&script_url), basic_headers.clone())?;
        if let Some(jwt) = find_jwt_token(&js_body) {
            return Ok(jwt);
        }
    }

    Err(nd_pdk::host::Error::msg(
        "Failed to find the JWT token in any Apple Music script asset",
    ))
}

fn fetch_lyrics_for_catalog_id(
    catalog_id: u64,
    jwt_token: &str,
    config: &LoadedConfig,
) -> Result<Vec<LyricsText>, nd_pdk::host::Error> {
    let mut basic_headers = HashMap::<String, String>::new();
    basic_headers.insert("User-Agent".to_string(), config.user_agent.to_string());
    basic_headers.insert("Authorization".to_string(), format!("Bearer {}", jwt_token));
    basic_headers.insert(
        "media-user-token".to_string(),
        config.media_token.to_string(),
    );
    basic_headers.insert(
        "Origin".to_string(),
        "https://beta.music.apple.com".to_string(),
    );
    basic_headers.insert(
        "Referer".to_string(),
        "https://beta.music.apple.com/".to_string(),
    );

    // https://amp-api.music.apple.com/v1/catalog/id/songs/6777110273/syllable-lyrics?l[lyrics]=en-gb&l[script]=en-Latn&extend=ttmlLocalizations
    let target_url = format!(
        "{}/v1/catalog/{}/songs/{}/syllable-lyrics?l[lyrics]={}&l[script]=und-Latn&extend=ttmlLocalizations",
        API_BASE_URL, config.account_country, config.translation_language, catalog_id
    );

    let req = nd_pdk::host::http::HTTPRequest {
        method: "GET".to_string(),
        url: target_url,
        headers: basic_headers.clone(),
        no_follow_redirects: false,
        body: Vec::new(),
        timeout_ms: 10_000,
    };

    let lyrics_resp = nd_pdk::host::http::send(req)?.ok_or(nd_pdk::host::Error::msg(format!(
        "No HTTP response found when fetching catalog ID: {}",
        catalog_id
    )))?;

    let lyrics_data = serde_json::from_slice::<SyllableLyricsResponse>(&lyrics_resp.body)?;

    let mapped_lyrics: Vec<LyricsText> = lyrics_data
        .data
        .iter()
        .filter_map(|data| match &data.attributes.ttml_localizations {
            Some(ttml) => Some(LyricsText {
                lang: "xxx".to_string(),
                text: ttml.to_string(),
            }),
            None => None,
        })
        .collect();

    Ok(mapped_lyrics)
}

fn fetch_lyrics_for_catalog_id_with_cache(
    catalog_id: u64,
    jwt_token: &str,
    config: &LoadedConfig,
) -> Result<Vec<LyricsText>, nd_pdk::host::Error> {
    let cache_key = format!("lyrics:{}", catalog_id);
    let ttl_seconds = config.cache_days * 24 * 60 * 60;
    if config.skip_cache {
        debug!(
            "Fetching {} lyrics from Apple Music (cache skip)...",
            catalog_id
        );
        let lyrics = fetch_lyrics_for_catalog_id(catalog_id, jwt_token, config)?;
        let json_data = serde_json::to_vec(&lyrics)?;
        nd_pdk::host::kvstore::set_with_ttl(&cache_key, json_data, ttl_seconds)?;
        Ok(lyrics)
    } else {
        let from_cache = nd_pdk::host::kvstore::get(&cache_key)?;
        if let Some(cached_data) = from_cache {
            debug!("Reading {} lyrics from cache...", catalog_id);
            let lyrics: Vec<LyricsText> = serde_json::from_slice(&cached_data)?;
            return Ok(lyrics);
        }
        debug!("Fetching {} lyrics from Apple Music...", catalog_id);
        let lyrics = fetch_lyrics_for_catalog_id(catalog_id, jwt_token, config)?;
        let json_data = serde_json::to_vec(&lyrics)?;
        nd_pdk::host::kvstore::set_with_ttl(&cache_key, json_data, ttl_seconds)?;
        Ok(lyrics)
    }
}
