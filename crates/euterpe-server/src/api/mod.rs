mod downloads;
mod health;
mod library;
mod qobuz;
mod server;

pub use downloads::{
    CreateDownloadRequest, CreateDownloadResponse, DownloadJob, DownloadJobListResponse,
    DownloadJobStatus, DownloadJobType, JobProgressEvent,
};
pub use health::{ErrorBody, ErrorResponse, HealthResponse};
pub use qobuz::{
    QobuzAccountListItem, QobuzAccountsListResponse, QobuzConnectionStatusResponse,
    QobuzFavoriteItem, QobuzFavoritesListResponse, QobuzFavoritesMutateRequest,
    QobuzOAuthStartResponse, QobuzSyncLatestResponse, QobuzSyncResponse, QobuzSyncRunSummary,
    QobuzTestLoginRequest, QobuzTestLoginResponse,
};
pub use library::{
    LibraryAlbumDetailResponse, LibraryAlbumItem, LibraryAlbumListResponse,
    LibraryScanLatestResponse, LibraryScanRunSummary, LibraryScanStartResponse,
    LibraryTrackDetailResponse, LibraryTrackItem, LibraryTrackTagsPatchRequest,
    ScanProgressEvent,
};
pub use server::ServerInfoResponse;
