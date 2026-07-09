use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct SyllableAttributes {
    #[serde(rename = "ttmlLocalizations", default)]
    pub(super) ttml_localizations: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub(super) struct SyllableData {
    pub(super) id: String,
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) attributes: SyllableAttributes,
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct SyllableLyricsResponse {
    pub(super) data: Vec<SyllableData>,
}
