export type SortOrder = "asc" | "desc";

export type KeysetListResponse<T> = {
  items: T[];
  next_cursor?: string | null;
  has_more: boolean;
};

export type KeysetListParams = {
  limit?: number;
  sort?: string;
  order?: SortOrder;
  cursor?: string | null;
};

/** Append defined query fields (skips null/undefined/empty string). */
export function appendKeysetParams(
  params: URLSearchParams,
  fields: Record<string, string | number | boolean | null | undefined>,
) {
  for (const [key, value] of Object.entries(fields)) {
    if (value === undefined || value === null) continue;
    if (typeof value === "string" && value.trim() === "") continue;
    if (typeof value === "boolean") {
      params.set(key, value ? "true" : "false");
    } else {
      params.set(key, String(value));
    }
  }
}
