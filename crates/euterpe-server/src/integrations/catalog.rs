use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationType {
    TagSource,
}

impl IntegrationType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TagSource => "tag_source",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "tag_source" => Some(Self::TagSource),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrationProvider {
    MusicBrainz,
    Discogs,
    Gnudb,
    Tracktype,
}

impl IntegrationProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MusicBrainz => "musicbrainz",
            Self::Discogs => "discogs",
            Self::Gnudb => "gnudb",
            Self::Tracktype => "tracktype",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "musicbrainz" => Some(Self::MusicBrainz),
            "discogs" => Some(Self::Discogs),
            "gnudb" => Some(Self::Gnudb),
            "tracktype" => Some(Self::Tracktype),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ConfigFieldSchema {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub required: bool,
    pub secret: bool,
    pub placeholder: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IntegrationCatalogEntry {
    pub provider: String,
    pub integration_type: String,
    pub label: String,
    pub description: String,
    pub requires_master_key: bool,
    pub config_schema: Vec<ConfigFieldSchema>,
}

pub fn catalog_entries(type_filter: Option<IntegrationType>) -> Vec<IntegrationCatalogEntry> {
    let all = vec![
        musicbrainz_entry(),
        discogs_entry(),
        gnudb_entry(),
        tracktype_entry(),
    ];
    match type_filter {
        None => all,
        Some(IntegrationType::TagSource) => all,
    }
}

fn musicbrainz_entry() -> IntegrationCatalogEntry {
    IntegrationCatalogEntry {
        provider: IntegrationProvider::MusicBrainz.as_str().into(),
        integration_type: IntegrationType::TagSource.as_str().into(),
        label: "MusicBrainz".into(),
        description: "Open music encyclopedia (release search by artist/album).".into(),
        requires_master_key: false,
        config_schema: vec![ConfigFieldSchema {
            key: "contact".into(),
            label: "Contact email".into(),
            field_type: "string".into(),
            required: true,
            secret: false,
            placeholder: Some("you@example.com".into()),
        }],
    }
}

fn discogs_entry() -> IntegrationCatalogEntry {
    IntegrationCatalogEntry {
        provider: IntegrationProvider::Discogs.as_str().into(),
        integration_type: IntegrationType::TagSource.as_str().into(),
        label: "Discogs".into(),
        description: "Community-built music database and marketplace.".into(),
        requires_master_key: true,
        config_schema: vec![ConfigFieldSchema {
            key: "token".into(),
            label: "Personal access token".into(),
            field_type: "string".into(),
            required: true,
            secret: true,
            placeholder: None,
        }],
    }
}

fn gnudb_entry() -> IntegrationCatalogEntry {
    IntegrationCatalogEntry {
        provider: IntegrationProvider::Gnudb.as_str().into(),
        integration_type: IntegrationType::TagSource.as_str().into(),
        label: "GnuDB".into(),
        description: "FreeDB-compatible metadata (TOC / track durations).".into(),
        requires_master_key: false,
        config_schema: vec![ConfigFieldSchema {
            key: "server_base".into(),
            label: "Server base URL".into(),
            field_type: "string".into(),
            required: false,
            secret: false,
            placeholder: Some("http://gnudb.gnudb.org:80".into()),
        }],
    }
}

fn tracktype_entry() -> IntegrationCatalogEntry {
    IntegrationCatalogEntry {
        provider: IntegrationProvider::Tracktype.as_str().into(),
        integration_type: IntegrationType::TagSource.as_str().into(),
        label: "TrackType.org".into(),
        description: "Supplementary metadata lookup.".into(),
        requires_master_key: false,
        config_schema: vec![
            ConfigFieldSchema {
                key: "api_base".into(),
                label: "API base URL".into(),
                field_type: "string".into(),
                required: false,
                secret: false,
                placeholder: Some("https://tracktype.org".into()),
            },
            ConfigFieldSchema {
                key: "api_key".into(),
                label: "API key".into(),
                field_type: "string".into(),
                required: false,
                secret: true,
                placeholder: None,
            },
        ],
    }
}

pub fn default_display_name(provider: IntegrationProvider) -> &'static str {
    match provider {
        IntegrationProvider::MusicBrainz => "MusicBrainz",
        IntegrationProvider::Discogs => "Discogs",
        IntegrationProvider::Gnudb => "GnuDB",
        IntegrationProvider::Tracktype => "TrackType.org",
    }
}
