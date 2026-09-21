const CONTROLLER_NAME = "JOCKY Controller" as const;

export interface HealthResponse {
  readonly status: "ok";
}

export interface VersionResponse {
  readonly name: typeof CONTROLLER_NAME;
  readonly version: string;
}

export interface ControllerSnapshot {
  readonly health: HealthResponse;
  readonly version: VersionResponse;
}

export interface ControllerClient {
  getStatus(signal?: AbortSignal): Promise<ControllerSnapshot>;
}

export type ControllerClientErrorCode =
  "aborted" | "http" | "invalid-json" | "invalid-response" | "network";

export class ControllerClientError extends Error {
  constructor(
    readonly code: ControllerClientErrorCode,
    message: string,
    readonly status: number | null = null,
  ) {
    super(message);
    this.name = "ControllerClientError";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseHealth(value: unknown): HealthResponse {
  if (!isRecord(value) || value.status !== "ok") {
    throw new ControllerClientError(
      "invalid-response",
      "Controller health response must have status 'ok'.",
    );
  }

  return { status: "ok" };
}

function parseVersion(value: unknown): VersionResponse {
  if (
    !isRecord(value) ||
    value.name !== CONTROLLER_NAME ||
    typeof value.version !== "string" ||
    value.version.trim().length === 0
  ) {
    throw new ControllerClientError(
      "invalid-response",
      "Controller version response is malformed.",
    );
  }

  return {
    name: CONTROLLER_NAME,
    version: value.version,
  };
}

function isAbortError(value: unknown): boolean {
  return isRecord(value) && value.name === "AbortError";
}

async function requestJson(
  fetcher: typeof fetch,
  url: string,
  signal?: AbortSignal,
): Promise<unknown> {
  const request: RequestInit = {
    headers: { Accept: "application/json" },
    method: "GET",
  };

  if (signal !== undefined) {
    request.signal = signal;
  }

  let response: Response;
  try {
    response = await fetcher(url, request);
  } catch (error: unknown) {
    if (isAbortError(error)) {
      throw new ControllerClientError("aborted", "Controller request aborted.");
    }
    throw new ControllerClientError(
      "network",
      "Controller request could not be completed.",
    );
  }

  if (!response.ok) {
    throw new ControllerClientError(
      "http",
      `Controller returned HTTP ${response.status}.`,
      response.status,
    );
  }

  try {
    const body: unknown = await response.json();
    return body;
  } catch {
    throw new ControllerClientError(
      "invalid-json",
      "Controller returned invalid JSON.",
    );
  }
}

function normalizeBaseUrl(baseUrl: string): string {
  return baseUrl.trim().replace(/\/+$/, "");
}

export function createControllerClient(
  baseUrl: string,
  fetcher: typeof fetch = (input, init) => globalThis.fetch(input, init),
): ControllerClient {
  const normalizedBaseUrl = normalizeBaseUrl(baseUrl);

  return {
    async getStatus(signal?: AbortSignal): Promise<ControllerSnapshot> {
      const [health, version] = await Promise.all([
        requestJson(fetcher, `${normalizedBaseUrl}/health`, signal),
        requestJson(fetcher, `${normalizedBaseUrl}/version`, signal),
      ]);

      return {
        health: parseHealth(health),
        version: parseVersion(version),
      };
    },
  };
}

export function describeControllerError(error: ControllerClientError): string {
  switch (error.code) {
    case "http":
      return error.status === null
        ? "Controller returned an HTTP error."
        : `Controller returned HTTP ${error.status}.`;
    case "invalid-json":
    case "invalid-response":
      return "Controller returned an unexpected response.";
    case "aborted":
      return "Controller check was cancelled.";
    case "network":
      return "Controller could not be reached.";
  }
}

const configuredControllerUrl = import.meta.env.VITE_CONTROLLER_URL?.trim();

export const defaultControllerClient = createControllerClient(
  configuredControllerUrl || "http://localhost:8000",
);
