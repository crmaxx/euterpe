import type { components } from "./schema";

export type ErrorResponse = components["schemas"]["ErrorResponse"];

export class ApiClientError extends Error {
  readonly code: string;
  readonly status: number;

  constructor(status: number, body: ErrorResponse) {
    super(body.error.message);
    this.name = "ApiClientError";
    this.code = body.error.code;
    this.status = status;
  }
}
