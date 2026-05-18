import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  api,
  type CreateDownloadRequest,
  type DownloadJob,
  type LibraryAlbumItem,
  type QobuzFavoriteItem,
  type SortOrder,
} from "./client";
import { ApiClientError } from "./errors";
import type { KeysetListResponse } from "./keyset";
import { flattenKeysetPages, useKeysetList } from "./hooks/keyset";
import { getDefaultQuality } from "@/lib/quality";
import {
  fetchAlbumCoverBlobUrl,
  revokeAlbumCoverBlobs,
} from "@/features/library/albumCoverBlobCache";

export type FavoritesListQuery = {
  limit?: number;
  sort?: "title" | "artist" | "in_library";
  order?: SortOrder;
  q?: string;
  in_library?: boolean;
};

export type LibraryAlbumsListQuery = {
  limit?: number;
  sort?: "title" | "artist" | "year";
  order?: SortOrder;
  q?: string;
};

export const queryKeys = {
  serverInfo: ["serverInfo"] as const,
  qobuzConnection: ["qobuzConnection"] as const,
  syncLatest: ["syncLatest"] as const,
  scanLatest: ["scanLatest"] as const,
  favorites: (params: FavoritesListQuery) => ["favorites", params] as const,
  downloads: (status?: string) => ["downloads", status] as const,
  libraryAlbums: (params: LibraryAlbumsListQuery) =>
    ["libraryAlbums", params] as const,
  libraryAlbum: (id: number) => ["libraryAlbum", id] as const,
  libraryTrack: (id: number) => ["libraryTrack", id] as const,
  albumCover: (albumId: number, coverPath: string) =>
    ["albumCover", albumId, coverPath] as const,
  integrations: (type?: string) => ["integrations", type ?? "all"] as const,
  integrationsCatalog: (type?: string) =>
    ["integrationsCatalog", type ?? "all"] as const,
};

export function useServerInfo() {
  return useQuery({
    queryKey: queryKeys.serverInfo,
    queryFn: api.serverInfo,
  });
}

export function useSyncLatest() {
  return useQuery({
    queryKey: queryKeys.syncLatest,
    queryFn: api.syncLatest,
    refetchInterval: 30_000,
  });
}

export function useScanLatest() {
  return useQuery({
    queryKey: queryKeys.scanLatest,
    queryFn: api.libraryScanLatest,
    refetchInterval: (query) =>
      query.state.error instanceof ApiClientError &&
      query.state.error.status === 401
        ? false
        : 5_000,
  });
}

export function useLibraryAlbumsKeyset(params: LibraryAlbumsListQuery = {}) {
  return useKeysetList<LibraryAlbumItem, LibraryAlbumsListQuery>({
    queryKey: queryKeys.libraryAlbums(params),
    params,
    queryFn: (p) => api.libraryAlbums(p),
  });
}

export function useLibraryAlbum(id: number | null) {
  return useQuery({
    queryKey: queryKeys.libraryAlbum(id ?? 0),
    queryFn: () => api.libraryAlbum(id!),
    enabled: id != null,
  });
}

export function useLibraryTrack(id: number | null) {
  return useQuery({
    queryKey: queryKeys.libraryTrack(id ?? 0),
    queryFn: () => api.libraryTrack(id!),
    enabled: id != null,
  });
}

export function useStartLibraryScan() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.startLibraryScan,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.scanLatest });
      void qc.invalidateQueries({ queryKey: ["libraryAlbums"] });
    },
  });
}

export function useAlbumCoverBlobUrl(
  albumId: number,
  coverPath?: string | null,
) {
  const path = coverPath?.trim() ?? "";
  return useQuery({
    queryKey: queryKeys.albumCover(albumId, path),
    queryFn: ({ signal }) => fetchAlbumCoverBlobUrl(albumId, path, signal),
    enabled: albumId > 0 && path.length > 0,
    staleTime: Infinity,
    gcTime: 60 * 60 * 1000,
    retry: 2,
  });
}

export function useUploadLibraryAlbumCover() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ albumId, file }: { albumId: number; file: File }) =>
      api.uploadLibraryAlbumCover(albumId, file, file.type),
    onSuccess: (_data, vars) => {
      revokeAlbumCoverBlobs(vars.albumId);
      void qc.invalidateQueries({ queryKey: ["albumCover", vars.albumId] });
      void qc.invalidateQueries({
        queryKey: queryKeys.libraryAlbum(vars.albumId),
      });
      void qc.invalidateQueries({ queryKey: ["libraryAlbums"] });
    },
  });
}

export function usePatchTrackTags() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: number;
      body: Parameters<typeof api.patchTrackTags>[1];
    }) => api.patchTrackTags(id, body),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.libraryTrack(vars.id) });
      void qc.invalidateQueries({ queryKey: ["libraryAlbums"] });
      void qc.invalidateQueries({ queryKey: ["libraryAlbum"] });
    },
  });
}

export function useIntegrations(type: "tag_source" = "tag_source") {
  return useQuery({
    queryKey: queryKeys.integrations(type),
    queryFn: () => api.listIntegrations(type),
  });
}

export function useIntegrationsCatalog(type: "tag_source" = "tag_source") {
  return useQuery({
    queryKey: queryKeys.integrationsCatalog(type),
    queryFn: () => api.integrationsCatalog(type),
  });
}

export function useCreateIntegration() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.createIntegration,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["integrations"] });
    },
  });
}

export function usePatchIntegration() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      id,
      body,
    }: {
      id: number;
      body: Parameters<typeof api.patchIntegration>[1];
    }) => api.patchIntegration(id, body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["integrations"] });
    },
  });
}

export function useDeleteIntegration() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.deleteIntegration,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["integrations"] });
    },
  });
}

export function useAlbumMetadataLookup() {
  return useMutation({
    mutationFn: ({
      albumId,
      integrationId,
      page = 1,
    }: {
      albumId: number;
      integrationId: number;
      page?: number;
    }) =>
      api.albumMetadataLookup(albumId, {
        integration_id: integrationId,
        page,
      }),
  });
}

export function useAlbumMetadataApply() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({
      albumId,
      integrationId,
      candidateId,
    }: {
      albumId: number;
      integrationId: number;
      candidateId: string;
    }) =>
      api.albumMetadataApply(albumId, {
        integration_id: integrationId,
        candidate_id: candidateId,
      }),
    onSuccess: (_data, vars) => {
      void qc.invalidateQueries({ queryKey: ["libraryAlbum", vars.albumId] });
      void qc.invalidateQueries({ queryKey: ["libraryAlbums"] });
      void qc.invalidateQueries({ queryKey: ["libraryTrack"] });
    },
  });
}

export function useFavoritesKeyset(params: FavoritesListQuery = {}) {
  return useKeysetList<QobuzFavoriteItem, FavoritesListQuery>({
    queryKey: queryKeys.favorites(params),
    params,
    queryFn: (p) => api.favorites(p),
  });
}

function favoritesFilterKey(params: FavoritesListQuery): string {
  return JSON.stringify({
    sort: params.sort ?? "title",
    order: params.order ?? "asc",
    q: params.q ?? "",
    in_library: params.in_library ?? null,
  });
}

/** Favorites table: single query page + manual load-more (safe for search). */
export function useFavoritesList(params: FavoritesListQuery = {}) {
  const filterKey = favoritesFilterKey(params);
  const [extraPagesState, setExtraPagesState] = useState<{
    filterKey: string;
    pages: KeysetListResponse<QobuzFavoriteItem>[];
  }>(() => ({ filterKey, pages: [] }));
  const [loadingMore, setLoadingMore] = useState(false);

  if (extraPagesState.filterKey !== filterKey) {
    setExtraPagesState({ filterKey, pages: [] });
  }
  const extraPages = extraPagesState.pages;

  const pageQuery = useQuery({
    queryKey: [...queryKeys.favorites(params), filterKey],
    queryFn: () => api.favorites(params),
    placeholderData: (previous, previousQuery) => {
      if (!previous || !previousQuery) return undefined;
      const prevFilterKey = previousQuery.queryKey.at(-1);
      return prevFilterKey === filterKey ? keepPreviousData(previous) : undefined;
    },
  });

  const items = useMemo(() => {
    const first = pageQuery.data?.items ?? [];
    if (extraPages.length === 0) return first;
    return [...first, ...extraPages.flatMap((p) => p.items)];
  }, [pageQuery.data, extraPages]);

  const hasMore =
    extraPages.length > 0
      ? (extraPages.at(-1)?.has_more ?? false)
      : (pageQuery.data?.has_more ?? false);

  const fetchNextPage = useCallback(async () => {
    const cursor =
      extraPages.length > 0
        ? extraPages.at(-1)?.next_cursor
        : pageQuery.data?.next_cursor;
    if (!cursor || !hasMore || loadingMore) return;
    setLoadingMore(true);
    try {
      const page = await api.favorites({ ...params, cursor });
      setExtraPagesState((prev) => ({
        filterKey,
        pages: [...(prev.filterKey === filterKey ? prev.pages : []), page],
      }));
    } finally {
      setLoadingMore(false);
    }
  }, [
    params,
    filterKey,
    extraPages,
    pageQuery.data?.next_cursor,
    hasMore,
    loadingMore,
  ]);

  return {
    items,
    isPending: pageQuery.isPending && items.length === 0,
    isFetching: pageQuery.isFetching && !loadingMore,
    hasNextPage: hasMore,
    fetchNextPage,
    isFetchingNextPage: loadingMore,
  };
}

/** Flattened favorites for queue titles and other consumers. */
export function useFavoritesFlat(params: FavoritesListQuery = { limit: 500 }) {
  const query = useFavoritesKeyset(params);
  const { hasNextPage, isFetchingNextPage, fetchNextPage, dataUpdatedAt } = query;
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) {
      void fetchNextPage();
    }
  }, [hasNextPage, isFetchingNextPage, fetchNextPage, dataUpdatedAt]);
  return {
    ...query,
    items: flattenKeysetPages(query.data),
  };
}

export function useDownloads(status?: string) {
  const query = useKeysetList<
    DownloadJob,
    { status?: string; limit: number; sort: string; order: "desc" }
  >({
    queryKey: queryKeys.downloads(status),
    params: { status, limit: 100, sort: "id", order: "desc" as const },
    queryFn: (p) => api.downloads(p),
    refetchInterval: 3_000,
  });
  const { hasNextPage, isFetchingNextPage, fetchNextPage, dataUpdatedAt } = query;
  useEffect(() => {
    if (hasNextPage && !isFetchingNextPage) {
      void fetchNextPage();
    }
  }, [hasNextPage, isFetchingNextPage, fetchNextPage, dataUpdatedAt]);
  return {
    ...query,
    data: query.data
      ? {
          items: flattenKeysetPages(query.data),
          next_cursor: query.data.pages.at(-1)?.next_cursor ?? null,
          has_more: query.hasNextPage ?? false,
        }
      : undefined,
    isLoading: query.isLoading,
  };
}

export function useQobuzSync() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.sync,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["favorites"] });
      void qc.invalidateQueries({ queryKey: queryKeys.syncLatest });
    },
  });
}

export function useQobuzConnection() {
  return useQuery({
    queryKey: queryKeys.qobuzConnection,
    queryFn: api.qobuzConnection,
  });
}

export function useQobuzOAuthStart() {
  return useMutation({
    mutationFn: api.qobuzOAuthStart,
    // Connection refreshes on return (?qobuz=connected) after redirect to Qobuz.
  });
}

export function useQobuzLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.qobuzLogout,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.qobuzConnection });
      void qc.invalidateQueries({ queryKey: queryKeys.serverInfo });
    },
  });
}

export function useRemoveFavorites() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (albumIds: number[]) => api.removeFavorites(albumIds),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["favorites"] });
    },
  });
}

export function useCreateDownloadByUrl() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (url: string) =>
      api.createDownloadByUrl(url, getDefaultQuality()),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    },
  });
}

export function useCreateDownload() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateDownloadRequest) => api.createDownload(body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    },
  });
}

export function useCancelDownload() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.cancelDownload(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    },
  });
}

export function usePurgeFinishedDownloads() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.purgeFinishedDownloads(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    },
  });
}

export function usePurgeDownload() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.purgeDownload(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["downloads"] });
    },
  });
}
