import { describe, expect, it, vi } from "vitest";

import {
  ControllerClientError,
  createControllerClient,
  describeControllerError,
} from "../src/api/controller";

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    headers: { "Content-Type": "application/json" },
    status,
  });
}

function validResponseFor(url: string): Response {
  if (url.endsWith("/health")) {
    return jsonResponse({ status: "ok" });
  }
  return jsonResponse({ name: "JOCKY Controller", version: "0.0.0" });
}

describe("createControllerClient", () => {
  it("loads and validates controller health and version", async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) =>
      validResponseFor(String(input)),
    );
    const client = createControllerClient("http://controller.test/", fetcher);

    await expect(client.getStatus()).resolves.toEqual({
      health: { status: "ok" },
      version: { name: "JOCKY Controller", version: "0.0.0" },
    });
    expect(fetcher).toHaveBeenCalledTimes(2);
    expect(fetcher).toHaveBeenCalledWith(
      "http://controller.test/health",
      expect.objectContaining({
        headers: { Accept: "application/json" },
        method: "GET",
      }),
    );
    expect(fetcher).toHaveBeenCalledWith(
      "http://controller.test/version",
      expect.objectContaining({ method: "GET" }),
    );
  });

  it.each([
    ["unknown health status", { status: "degraded" }, null],
    ["missing controller name", { status: "ok" }, { version: "0.0.0" }],
    [
      "unknown controller name",
      { status: "ok" },
      { name: "Another service", version: "0.0.0" },
    ],
    ["missing version", { status: "ok" }, { name: "JOCKY Controller" }],
  ])("rejects a response with %s", async (_case, healthBody, versionBody) => {
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (String(input).endsWith("/health")) {
        return jsonResponse(healthBody);
      }
      return jsonResponse(
        versionBody ?? { name: "JOCKY Controller", version: "0.0.0" },
      );
    });
    const client = createControllerClient("http://controller.test", fetcher);

    await expect(client.getStatus()).rejects.toMatchObject({
      code: "invalid-response",
    } satisfies Partial<ControllerClientError>);
  });

  it("returns a typed error for a non-success response", async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (String(input).endsWith("/health")) {
        return jsonResponse({ detail: "unavailable" }, 503);
      }
      return validResponseFor(String(input));
    });
    const client = createControllerClient("http://controller.test", fetcher);

    await expect(client.getStatus()).rejects.toMatchObject({
      code: "http",
      status: 503,
    } satisfies Partial<ControllerClientError>);
  });

  it("returns a typed error when the controller cannot be reached", async () => {
    const fetcher = vi.fn<typeof fetch>(async () => {
      throw new TypeError("connection refused");
    });
    const client = createControllerClient("http://controller.test", fetcher);

    await expect(client.getStatus()).rejects.toMatchObject({
      code: "network",
    } satisfies Partial<ControllerClientError>);
  });

  it("returns a typed cancellation error and forwards the abort signal", async () => {
    const abortController = new AbortController();
    const fetcher = vi.fn<typeof fetch>(async () => {
      throw new DOMException("request cancelled", "AbortError");
    });
    const client = createControllerClient("http://controller.test", fetcher);

    await expect(
      client.getStatus(abortController.signal),
    ).rejects.toMatchObject({
      code: "aborted",
    } satisfies Partial<ControllerClientError>);
    expect(fetcher).toHaveBeenCalledWith(
      "http://controller.test/health",
      expect.objectContaining({ signal: abortController.signal }),
    );
  });

  it("returns a typed error for malformed JSON", async () => {
    const fetcher = vi.fn<typeof fetch>(async (input) => {
      if (String(input).endsWith("/health")) {
        return new Response("not-json", {
          headers: { "Content-Type": "application/json" },
          status: 200,
        });
      }
      return validResponseFor(String(input));
    });
    const client = createControllerClient("http://controller.test", fetcher);

    await expect(client.getStatus()).rejects.toMatchObject({
      code: "invalid-json",
    } satisfies Partial<ControllerClientError>);
  });
});

describe("describeControllerError", () => {
  it.each([
    ["network", null, "Controller could not be reached."],
    ["aborted", null, "Controller check was cancelled."],
    ["invalid-json", null, "Controller returned an unexpected response."],
    ["invalid-response", null, "Controller returned an unexpected response."],
    ["http", 503, "Controller returned HTTP 503."],
  ] as const)(
    "maps %s errors to safe operator text",
    (code, status, expected) => {
      const error = new ControllerClientError(code, "internal detail", status);

      expect(describeControllerError(error)).toBe(expected);
      expect(describeControllerError(error)).not.toContain("internal detail");
    },
  );
});
