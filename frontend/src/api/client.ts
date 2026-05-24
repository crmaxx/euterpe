import { getAdminToken, notifyAdminUnauthorized } from "@/lib/auth";
import { ApiClientError, type ErrorResponse } from "./errors";
import { appendKeysetParams, type KeysetListParams, type SortOrder } from "./keyset";
import type { components } from "./schema";

export type { KeysetListParams, SortOrder };
export type { KeysetListResponse } from "./keyset";

export type ServerInfoResponse = components["schemas"]["ServerInfoResponse"];
export type QobuzSyncLatestResponse =
  components["schemas"]["QobuzSyncLatestResponse"];
export type QobuzFavoritesListResponse =
  components["schemas"]["QobuzFavoritesListResponse"];
export type QobuzFavoriteItem = components["schemas"]["QobuzFavoriteItem"];
export type QobuzSyncResponse = components["schemas"]["QobuzSyncResponse"];
export type QobuzTestLoginRequest =
  components["schemas"]["QobuzTestLoginRequest"];
export type QobuzTestLoginResponse =
  components["schemas"]["QobuzTestLoginResponse"];
export type QobuzOAuthStartResponse =
  components["schemas"]["QobuzOAuthStartResponse"];
export type QobuzConnectionStatusResponse =
  components["schemas"]["QobuzConnectionStatusResponse"];
export type DownloadJobListResponse =
  components["schemas"]["DownloadJobListResponse"];
export type DownloadJob = components["schemas"]["DownloadJob"];
export type TorrentJobDetail = components["schemas"]["TorrentJobDetail"];
export type CreateDownloadRequest =
  components["schemas"]["CreateDownloadRequest"];
export type CreateDownloadResponse =
  components["schemas"]["CreateDownloadResponse"];
export type JobProgressEvent = components["schemas"]["JobProgressEvent"];
export type TorrentInspectResponse =
  components["schemas"]["TorrentInspectResponse"];
export type TorrentInspectFile = components["schemas"]["TorrentInspectFile"];
export type TorrentConfirmRequest =
  components["schemas"]["TorrentConfirmRequest"];
export type TorrentPostDownloadOptions =
  components["schemas"]["TorrentPostDownloadOptions"];
export type TorrentSettings = components["schemas"]["TorrentSettings"];
export type TorrentSettingsPatch =
  components["schemas"]["TorrentSettingsPatch"];
export type TorrentSettingsResponse =
  components["schemas"]["TorrentSettingsResponse"];
export type DownloadSource = components["schemas"]["DownloadSource"];
export type ScanProgressEvent = components["schemas"]["ScanProgressEvent"];
export type LibraryScanLatestResponse =
  components["schemas"]["LibraryScanLatestResponse"];
export type LibraryScanStartResponse =
  components["schemas"]["LibraryScanStartResponse"];
export type LibraryAlbumListResponse =
  components["schemas"]["LibraryAlbumListResponse"];
export type LibraryAlbumItem = components["schemas"]["LibraryAlbumItem"];
export type LibraryAlbumDetailResponse =
  components["schemas"]["LibraryAlbumDetailResponse"];
export type LibraryTrackDetailResponse =
  components["schemas"]["LibraryTrackDetailResponse"];
export type LibraryTrackTagsPatchRequest =
  components["schemas"]["LibraryTrackTagsPatchRequest"];
export type LibraryAlbumTagsPatchRequest =
  components["schemas"]["LibraryAlbumTagsPatchRequest"];
export type IntegrationListItem = components["schemas"]["IntegrationListItem"];
export type IntegrationsListResponse =
  components["schemas"]["IntegrationsListResponse"];
export type IntegrationsCatalogResponse =
  components["schemas"]["IntegrationsCatalogResponse"];
export type IntegrationCatalogEntry =
  components["schemas"]["IntegrationCatalogEntry"];
export type IntegrationCreateRequest =
  components["schemas"]["IntegrationCreateRequest"];
export type IntegrationPatchRequest =
  components["schemas"]["IntegrationPatchRequest"];
export type IntegrationResponse = components["schemas"]["IntegrationResponse"];
export type MetadataCandidate = components["schemas"]["MetadataCandidate"];
export type AlbumMetadataLookupRequest =
  components["schemas"]["AlbumMetadataLookupRequest"];
export type AlbumMetadataLookupResponse =
  components["schemas"]["AlbumMetadataLookupResponse"];
export type AlbumMetadataApplyRequest =
  components["schemas"]["AlbumMetadataApplyRequest"];
export type AlbumMetadataApplyResponse =
  components["schemas"]["AlbumMetadataApplyResponse"];
export type AlbumCoverUploadResponse =
  components["schemas"]["AlbumCoverUploadResponse"];
export type UiPreferences = components["schemas"]["UiPreferences"];
export type UiPreferencesPatch = components["schemas"]["UiPreferencesPatch"];
export type UiPreferencesResponse =
  components["schemas"]["UiPreferencesResponse"];
export type ConverterSettings = components["schemas"]["ConverterSettings"];
export type ConverterSettingsPatch =
  components["schemas"]["ConverterSettingsPatch"];
export type ConverterSettingsResponse =
  components["schemas"]["ConverterSettingsResponse"];
export type LibraryScanSettings = components["schemas"]["LibraryScanSettings"];
export type LibraryScanSettingsPatch =
  components["schemas"]["LibraryScanSettingsPatch"];
export type LibraryScanSettingsResponse =
  components["schemas"]["LibraryScanSettingsResponse"];
export type DownloadsSettings = components["schemas"]["DownloadsSettings"];
export type DownloadsSettingsPatch =
  components["schemas"]["DownloadsSettingsPatch"];
export type DownloadsSettingsResponse =
  components["schemas"]["DownloadsSettingsResponse"];
export type ConvertAlbumResponse =
  components["schemas"]["ConvertAlbumResponse"];
export type ConvertJobResponse = components["schemas"]["ConvertJobResponse"];
export type ConvertJobSummary = components["schemas"]["ConvertJobSummary"];
export type CueAlbumResponse = components["schemas"]["CueAlbumResponse"];
export type CueDocument = components["schemas"]["CueDocument"];
export type CueValidateRequest = components["schemas"]["CueValidateRequest"];
export type CueValidationResponse =
  components["schemas"]["CueValidationResponse"];
export type CueSplitRequest = components["schemas"]["CueSplitRequest"];
export type CueSplitResponse = components["schemas"]["CueSplitResponse"];
export type CueJobResponse = components["schemas"]["CueJobResponse"];

export type ConvertFileProgress = {
  path: string;
  status: string;
  /** Per-file encode progress 0–100 while status is `running`. */
  progress_pct?: number | null;
  error?: string | null;
};

/** SSE `convert_progress` payload (not in OpenAPI event schemas). */
export type ConvertProgressEvent = {
  job_id: number;
  album_id: number;
  status: string;
  files_total: number;
  files_done: number;
  progress_pct: number;
  files?: ConvertFileProgress[];
  error_message?: string | null;
};

/** Max cover upload size (must match server `MAX_ALBUM_COVER_BYTES`). */
export const MAX_ALBUM_COVER_BYTES = 20 * 1024 * 1024;

const API_BASE = "/api/v1";

/** URL for Howler / `<audio>` (same-origin; admin token in query when set). */
export function libraryTrackStreamUrl(trackId: number): string {
  const base = `${API_BASE}/library/tracks/${trackId}/stream`;
  const token = getAdminToken();
  if (!token) {
    return base;
  }
  const params = new URLSearchParams({ access_token: token });
  return `${base}?${params.toString()}`;
}

export async function fetchJson<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const headers = new Headers(init?.headers);
  if (
    !headers.has("Content-Type") &&
    init?.body &&
    !(init.body instanceof FormData)
  ) {
    headers.set("Content-Type", "application/json");
  }
  const token = getAdminToken();
  if (token) {
    headers.set("Authorization", `Bearer ${token}`);
  }

  const response = await fetch(`${API_BASE}${path}`, { ...init, headers });
  if (response.status === 204) {
    return undefined as T;
  }

  const text = await response.text();
  const json = text ? (JSON.parse(text) as unknown) : null;

  if (!response.ok) {
    const errBody =
      (json as ErrorResponse) ?? {
        error: { code: "UNKNOWN", message: response.statusText },
      };
    if (response.status === 401 && errBody.error.code === "UNAUTHORIZED") {
      notifyAdminUnauthorized();
    }
    throw new ApiClientError(response.status, errBody);
  }

  return json as T;
}

export const api = {
  serverInfo: () => fetchJson<ServerInfoResponse>("/server/info"),

  syncLatest: () => fetchJson<QobuzSyncLatestResponse>("/qobuz/sync/latest"),

  favorites: (
    params: KeysetListParams & {
      q?: string;
      in_library?: boolean;
    } = {},
  ) => {
    const search = new URLSearchParams({ type: "album" });
    appendKeysetParams(search, {
      limit: params.limit ?? 50,
      sort: params.sort ?? "title",
      order: params.order ?? "asc",
      cursor: params.cursor ?? undefined,
      q: params.q,
      in_library: params.in_library,
    });
    return fetchJson<QobuzFavoritesListResponse>(`/qobuz/favorites?${search}`);
  },

  sync: () =>
    fetchJson<QobuzSyncResponse>("/qobuz/sync", { method: "POST" }),

  testLogin: (body: QobuzTestLoginRequest) =>
    fetchJson<QobuzTestLoginResponse>("/qobuz/test-login", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  qobuzOAuthStart: () =>
    fetchJson<QobuzOAuthStartResponse>("/qobuz/oauth/start"),

  qobuzConnection: () =>
    fetchJson<QobuzConnectionStatusResponse>("/qobuz/connection"),

  qobuzLogout: () =>
    fetchJson<void>("/qobuz/logout", { method: "POST" }),

  removeFavorites: (albumIds: number[]) =>
    fetchJson<void>("/qobuz/favorites", {
      method: "DELETE",
      body: JSON.stringify({ album_ids: albumIds }),
    }),

  createDownloadByUrl: (url: string, quality: number) =>
    fetchJson<CreateDownloadResponse>("/downloads/by-url", {
      method: "POST",
      body: JSON.stringify({ url, quality }),
    }),

  inspectTorrentMagnet: (magnet: string) =>
    fetchJson<TorrentInspectResponse>("/downloads/torrent/inspect", {
      method: "POST",
      body: JSON.stringify({ magnet }),
    }),

  inspectTorrentFile: (file: File) => {
    const form = new FormData();
    form.append("file", file);
    return fetchJson<TorrentInspectResponse>("/downloads/torrent/inspect/file", {
      method: "POST",
      body: form,
    });
  },

  confirmTorrentDownload: (body: TorrentConfirmRequest) =>
    fetchJson<CreateDownloadResponse>("/downloads/torrent/confirm", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  torrentSettings: () =>
    fetchJson<TorrentSettingsResponse>("/settings/torrent"),

  patchTorrentSettings: (body: TorrentSettingsPatch) =>
    fetchJson<TorrentSettingsResponse>("/settings/torrent", {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  uiSettings: () => fetchJson<UiPreferencesResponse>("/settings/ui"),

  patchUiSettings: (body: UiPreferencesPatch) =>
    fetchJson<UiPreferencesResponse>("/settings/ui", {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  converterSettings: () =>
    fetchJson<ConverterSettingsResponse>("/settings/converter"),

  patchConverterSettings: (body: ConverterSettingsPatch) =>
    fetchJson<ConverterSettingsResponse>("/settings/converter", {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  libraryScanSettings: () =>
    fetchJson<LibraryScanSettingsResponse>("/settings/library-scan"),

  patchLibraryScanSettings: (body: LibraryScanSettingsPatch) =>
    fetchJson<LibraryScanSettingsResponse>("/settings/library-scan", {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  downloadsSettings: () =>
    fetchJson<DownloadsSettingsResponse>("/settings/downloads"),

  patchDownloadsSettings: (body: DownloadsSettingsPatch) =>
    fetchJson<DownloadsSettingsResponse>("/settings/downloads", {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  postAlbumConvert: (albumId: number) =>
    fetchJson<ConvertAlbumResponse>(`/library/albums/${albumId}/convert`, {
      method: "POST",
    }),

  albumConvertLatest: (albumId: number) =>
    fetchJson<ConvertJobResponse>(`/library/albums/${albumId}/convert/latest`),

  convertJob: (jobId: number) =>
    fetchJson<ConvertJobResponse>(`/library/convert/jobs/${jobId}`),

  albumCue: (albumId: number, cuePath?: string) => {
    const q = cuePath ? `?cue_path=${encodeURIComponent(cuePath)}` : "";
    return fetchJson<CueAlbumResponse>(`/library/albums/${albumId}/cue${q}`);
  },

  validateAlbumCue: (albumId: number, body: CueValidateRequest) =>
    fetchJson<CueValidationResponse>(`/library/albums/${albumId}/cue/validate`, {
      method: "POST",
      body: JSON.stringify(body),
    }),

  splitAlbumCue: (albumId: number, body: CueSplitRequest) =>
    fetchJson<CueSplitResponse>(`/library/albums/${albumId}/cue/split`, {
      method: "POST",
      body: JSON.stringify(body),
    }),

  albumCueLatest: (albumId: number) =>
    fetchJson<CueJobResponse>(`/library/albums/${albumId}/cue/latest`),

  downloads: (
    params: KeysetListParams & { status?: string } = {},
  ) => {
    const search = new URLSearchParams();
    appendKeysetParams(search, {
      limit: params.limit ?? 100,
      sort: params.sort ?? "queue_position",
      order: params.order ?? "asc",
      cursor: params.cursor ?? undefined,
      status: params.status,
    });
    return fetchJson<DownloadJobListResponse>(`/downloads?${search}`);
  },

  patchDownloadPriority: (id: number, direction: "up" | "down") =>
    fetchJson<void>(`/downloads/${id}/priority`, {
      method: "PATCH",
      body: JSON.stringify({ direction }),
    }),

  retryDownload: (id: number) =>
    fetchJson<void>(`/downloads/${id}/retry`, { method: "POST" }),

  pauseDownload: (id: number) =>
    fetchJson<void>(`/downloads/${id}/pause`, { method: "POST" }),

  resumeDownload: (id: number) =>
    fetchJson<void>(`/downloads/${id}/resume`, { method: "POST" }),

  createDownload: (body: CreateDownloadRequest) =>
    fetchJson<CreateDownloadResponse>("/downloads", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  cancelDownload: (id: number) =>
    fetchJson<void>(`/downloads/${id}`, { method: "DELETE" }),

  purgeFinishedDownloads: () =>
    fetchJson<{ deleted: number }>("/downloads/purge", { method: "POST" }),

  purgeDownload: (id: number) =>
    fetchJson<void>(`/downloads/${id}?purge=1`, { method: "DELETE" }),

  libraryScanLatest: () =>
    fetchJson<LibraryScanLatestResponse>("/library/scan/latest"),

  startLibraryScan: (root?: string) => {
    const q =
      root != null && root.length > 0
        ? `?root=${encodeURIComponent(root)}`
        : "";
    return fetchJson<LibraryScanStartResponse>(`/library/scan${q}`, {
      method: "POST",
    });
  },

  cancelLibraryScan: (scanId: number) =>
    fetchJson<void>(`/library/scan/${scanId}`, { method: "DELETE" }),

  libraryAlbums: (params: KeysetListParams & { q?: string } = {}) => {
    const search = new URLSearchParams();
    appendKeysetParams(search, {
      limit: params.limit ?? 50,
      sort: params.sort ?? "title",
      order: params.order ?? "asc",
      cursor: params.cursor ?? undefined,
      q: params.q,
    });
    return fetchJson<LibraryAlbumListResponse>(`/library/albums?${search}`);
  },

  libraryAlbum: (id: number) =>
    fetchJson<LibraryAlbumDetailResponse>(`/library/albums/${id}`),

  uploadLibraryAlbumCover: (
    albumId: number,
    body: Blob,
    contentType: string,
  ) =>
    fetchJson<AlbumCoverUploadResponse>(`/library/albums/${albumId}/cover`, {
      method: "PUT",
      headers: { "Content-Type": contentType },
      body,
    }),

  libraryTrack: (id: number) =>
    fetchJson<LibraryTrackDetailResponse>(`/library/tracks/${id}`),

  patchTrackTags: (id: number, body: LibraryTrackTagsPatchRequest) =>
    fetchJson<LibraryTrackDetailResponse>(`/library/tracks/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  patchAlbumTags: (id: number, body: LibraryAlbumTagsPatchRequest) =>
    fetchJson<LibraryAlbumDetailResponse>(`/library/albums/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  listIntegrations: (type?: "tag_source") => {
    const search = type ? `?type=${type}` : "";
    return fetchJson<IntegrationsListResponse>(`/integrations${search}`);
  },

  integrationsCatalog: (type?: "tag_source") => {
    const search = type ? `?type=${type}` : "";
    return fetchJson<IntegrationsCatalogResponse>(`/integrations/catalog${search}`);
  },

  createIntegration: (body: IntegrationCreateRequest) =>
    fetchJson<IntegrationResponse>("/integrations", {
      method: "POST",
      body: JSON.stringify(body),
    }),

  patchIntegration: (id: number, body: IntegrationPatchRequest) =>
    fetchJson<IntegrationResponse>(`/integrations/${id}`, {
      method: "PATCH",
      body: JSON.stringify(body),
    }),

  deleteIntegration: (id: number) =>
    fetchJson<void>(`/integrations/${id}`, { method: "DELETE" }),

  albumMetadataLookup: (albumId: number, body: AlbumMetadataLookupRequest) =>
    fetchJson<AlbumMetadataLookupResponse>(
      `/library/albums/${albumId}/metadata/lookup`,
      { method: "POST", body: JSON.stringify(body) },
    ),

  albumMetadataApply: (albumId: number, body: AlbumMetadataApplyRequest) =>
    fetchJson<AlbumMetadataApplyResponse>(
      `/library/albums/${albumId}/metadata/apply`,
      { method: "POST", body: JSON.stringify(body) },
    ),
};

export function subscribeServerEvents(handlers: {
  onJobProgress?: (event: JobProgressEvent) => void;
  onScanProgress?: (event: ScanProgressEvent) => void;
  onConvertProgress?: (event: ConvertProgressEvent) => void;
}): EventSource {
  const token = getAdminToken();
  const eventsUrl =
    token != null && token.length > 0
      ? `${API_BASE}/events?access_token=${encodeURIComponent(token)}`
      : `${API_BASE}/events`;
  const source = new EventSource(eventsUrl);
  if (handlers.onJobProgress) {
    source.addEventListener("job_progress", (ev) => {
      handlers.onJobProgress?.(JSON.parse(ev.data) as JobProgressEvent);
    });
  }
  if (handlers.onScanProgress) {
    source.addEventListener("scan_progress", (ev) => {
      handlers.onScanProgress?.(JSON.parse(ev.data) as ScanProgressEvent);
    });
  }
  if (handlers.onConvertProgress) {
    source.addEventListener("convert_progress", (ev) => {
      handlers.onConvertProgress?.(JSON.parse(ev.data) as ConvertProgressEvent);
    });
  }
  return source;
}

/** @deprecated use subscribeServerEvents */
export function subscribeJobProgress(
  onEvent: (event: JobProgressEvent) => void,
): EventSource {
  return subscribeServerEvents({ onJobProgress: onEvent });
}
