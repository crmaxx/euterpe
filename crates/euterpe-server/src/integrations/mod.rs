pub mod apply;
pub mod catalog;
pub mod discogs;
pub mod gnudb;
pub mod musicbrainz;
pub mod provider;
pub mod registry;
pub mod tracktype;
pub mod types;

pub use catalog::{IntegrationCatalogEntry, IntegrationProvider, IntegrationType};
pub use types::{AlbumLookupResult, MetadataCandidate};
