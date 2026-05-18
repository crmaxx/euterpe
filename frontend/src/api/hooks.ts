import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type CreateDownloadRequest, type QobuzTestLoginRequest } from "./client";

export const queryKeys = {
  serverInfo: ["serverInfo"] as const,
  syncLatest: ["syncLatest"] as const,
  favorites: (page: number, limit: number) =>
    ["favorites", page, limit] as const,
  downloads: ["downloads"] as const,
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

export function useTestLogin() {
  return useMutation({
    mutationFn: (body: QobuzTestLoginRequest) => api.testLogin(body),
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
