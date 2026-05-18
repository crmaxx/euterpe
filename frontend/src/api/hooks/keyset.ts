import {
  useInfiniteQuery,
  type InfiniteData,
  type QueryKey,
} from "@tanstack/react-query";
import type { KeysetListResponse } from "@/api/keyset";

export function flattenKeysetPages<T>(
  data: InfiniteData<KeysetListResponse<T>> | undefined,
): T[] {
  return data?.pages.flatMap((p) => p.items) ?? [];
}

export function useKeysetList<T, P extends Record<string, unknown>>({
  queryKey,
  queryFn,
  params,
  enabled = true,
  refetchInterval,
}: {
  queryKey: QueryKey;
  queryFn: (params: P & { cursor?: string }) => Promise<KeysetListResponse<T>>;
  params: P;
  enabled?: boolean;
  refetchInterval?: number | false;
}) {
  return useInfiniteQuery<
    KeysetListResponse<T>,
    Error,
    InfiniteData<KeysetListResponse<T>>,
    QueryKey,
    string | undefined
  >({
    queryKey,
    queryFn: ({ pageParam }) =>
      queryFn({ ...params, cursor: pageParam as string | undefined }),
    initialPageParam: undefined as string | undefined,
    getNextPageParam: (last) =>
      last.has_more ? (last.next_cursor ?? undefined) : undefined,
    enabled,
    refetchInterval,
  });
}
