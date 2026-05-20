mod downloads;
mod health;
mod integrations;
pub mod keyset;
mod library;
mod qobuz;
mod server;
mod torrent;

pub use keyset::{KeysetPage, SortKeyKind, SortKeyValue, SortOrder};

pub use downloads::{
    CreateDownloadByUrlRequest, CreateDownloadRequest, CreateDownloadResponse, DownloadJob,
    DownloadJobListResponse, DownloadJobStatus, DownloadJobType, DownloadPurgeResponse,
    DownloadSource, JobProgressEvent, PatchDownloadPriorityRequest, TorrentEuterpePhase,
    TorrentJobDetail, TorrentLibrqbitState,
};
pub use health::{ErrorBody, ErrorResponse, HealthResponse};
pub use integrations::{
    AlbumMetadataApplyRequest, AlbumMetadataApplyResponse, AlbumMetadataLookupRequest,
    AlbumMetadataLookupResponse, IntegrationCreateRequest, IntegrationListItem,
    IntegrationPatchRequest, IntegrationResponse, IntegrationsCatalogResponse,
    IntegrationsListResponse,
};
pub use library::{
    AlbumCoverUploadResponse, LibraryAlbumDetailResponse, LibraryAlbumItem,
    LibraryAlbumListResponse, LibraryAlbumTagsPatchRequest, LibraryScanLatestResponse,
    LibraryScanRunSummary, LibraryScanStartResponse, LibraryTrackDetailResponse, LibraryTrackItem,
    LibraryTrackTagsPatchRequest, ScanProgressEvent,
};
pub use qobuz::{
    QobuzAccountListItem, QobuzAccountsListResponse, QobuzConnectionStatusResponse,
    QobuzFavoriteItem, QobuzFavoritesListResponse, QobuzFavoritesMutateRequest,
    QobuzOAuthStartResponse, QobuzSyncLatestResponse, QobuzSyncResponse, QobuzSyncRunSummary,
    QobuzTestLoginRequest, QobuzTestLoginResponse,
};
pub use server::ServerInfoResponse;
pub use torrent::{
    TorrentConfirmRequest, TorrentInspectFile, TorrentInspectMagnetRequest, TorrentInspectResponse,
    TorrentSettings, TorrentSettingsPatch, TorrentSettingsResponse,
};
