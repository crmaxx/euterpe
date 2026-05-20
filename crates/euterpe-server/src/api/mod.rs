mod downloads;
mod health;
mod integrations;
pub mod keyset;
mod library;
mod qobuz;
mod server;
mod torrent;

pub use keyset::{KeysetPage, SortOrder, SortKeyKind, SortKeyValue};

pub use downloads::{
    CreateDownloadByUrlRequest, CreateDownloadRequest, CreateDownloadResponse, DownloadJob,
    DownloadJobListResponse, DownloadJobStatus, DownloadJobType, DownloadPurgeResponse,
    DownloadSource, JobProgressEvent, PatchDownloadPriorityRequest, TorrentEuterpePhase,
    TorrentJobDetail, TorrentLibrqbitState,
};
pub use torrent::{
    TorrentConfirmRequest, TorrentInspectFile, TorrentInspectMagnetRequest, TorrentInspectResponse,
    TorrentSettings, TorrentSettingsPatch, TorrentSettingsResponse,
};
pub use health::{ErrorBody, ErrorResponse, HealthResponse};
pub use integrations::{
    AlbumMetadataApplyRequest, AlbumMetadataApplyResponse, AlbumMetadataLookupRequest,
    AlbumMetadataLookupResponse, IntegrationCreateRequest, IntegrationListItem,
    IntegrationPatchRequest, IntegrationResponse, IntegrationsCatalogResponse,
    IntegrationsListResponse,
};
pub use qobuz::{
    QobuzAccountListItem, QobuzAccountsListResponse, QobuzConnectionStatusResponse,
    QobuzFavoriteItem, QobuzFavoritesListResponse,
    QobuzFavoritesMutateRequest,
    QobuzOAuthStartResponse, QobuzSyncLatestResponse, QobuzSyncResponse, QobuzSyncRunSummary,
    QobuzTestLoginRequest, QobuzTestLoginResponse,
};
pub use library::{
    AlbumCoverUploadResponse, LibraryAlbumDetailResponse, LibraryAlbumItem,
    LibraryAlbumListResponse,
    LibraryScanLatestResponse, LibraryScanRunSummary, LibraryScanStartResponse,
    LibraryAlbumTagsPatchRequest, LibraryTrackDetailResponse, LibraryTrackItem,
    LibraryTrackTagsPatchRequest,
    ScanProgressEvent,
};
pub use server::ServerInfoResponse;
