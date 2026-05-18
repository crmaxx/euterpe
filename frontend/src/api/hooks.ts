import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type CreateDownloadRequest } from "./client";
import { ApiClientError } from "./errors";

export const queryKeys = {
  serverInfo: ["serverInfo"] as const,
  qobuzConnection: ["qobuzConnection"] as const,
  syncLatest: ["syncLatest"] as const,
  scanLatest: ["scanLatest"] as const,
  favorites: (page: number, limit: number) =>
    ["favorites", page, limit] as const,
  downloads: ["downloads"] as const,
  libraryAlbums: (page: number, limit: number, search: string) =>
    ["libraryAlbums", page, limit, search] as const,
  libraryAlbum: (id: number) => ["libraryAlbum", id] as const,
  libraryTrack: (id: number) => ["libraryTrack", id] as const,
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

export function useLibraryAlbums(page = 0, limit = 50, search = "") {
  return useQuery({
    queryKey: queryKeys.libraryAlbums(page, limit, search),
    queryFn: () => api.libraryAlbums(page, limit, search),
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

export function useFavorites(page = 0, limit = 50) {
  return useQuery({
    queryKey: queryKeys.favorites(page, limit),
    queryFn: () => api.favorites(page, limit),
  });
}

export function useDownloads() {
  return useQuery({
    queryKey: queryKeys.downloads,
    queryFn: () => api.downloads(),
    refetchInterval: 3_000,
  });
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
  const qc = useQueryClient();
  return useMutation({
    mutationFn: api.qobuzOAuthStart,
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.qobuzConnection });
      void qc.invalidateQueries({ queryKey: queryKeys.serverInfo });
    },
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

export function useCreateDownload() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateDownloadRequest) => api.createDownload(body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.downloads });
    },
  });
}

export function useCancelDownload() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.cancelDownload(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.downloads });
    },
  });
}

export function usePurgeFinishedDownloads() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.purgeFinishedDownloads(),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.downloads });
    },
  });
}

export function usePurgeDownload() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: number) => api.purgeDownload(id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.downloads });
    },
  });
}
