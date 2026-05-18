const STORAGE_KEY = "euterpe.adminToken";

/** Dispatched when the server rejects the stored admin token (401 UNAUTHORIZED). */
export const ADMIN_UNAUTHORIZED_EVENT = "euterpe:admin-unauthorized";

export function getAdminToken(): string | null {
  return sessionStorage.getItem(STORAGE_KEY);
}

export function setAdminToken(token: string) {
  sessionStorage.setItem(STORAGE_KEY, token);
}

export function clearAdminToken() {
  sessionStorage.removeItem(STORAGE_KEY);
}

export function notifyAdminUnauthorized() {
  clearAdminToken();
  window.dispatchEvent(new CustomEvent(ADMIN_UNAUTHORIZED_EVENT));
}
