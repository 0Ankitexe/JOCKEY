import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { App } from "../src/App";
import {
  ControllerClientError,
  type ControllerClient,
  type ControllerSnapshot,
} from "../src/api/controller";

const healthySnapshot: ControllerSnapshot = {
  health: { status: "ok" },
  version: { name: "JOCKY Controller", version: "0.0.0" },
};

function clientReturning(
  outcome: Promise<ControllerSnapshot>,
): ControllerClient {
  return { getStatus: vi.fn(() => outcome) };
}

describe("App status integration", () => {
  it("shows the bounded Version 0 scope while checking controller health", () => {
    const pending = new Promise<ControllerSnapshot>(() => undefined);

    render(<App client={clientReturning(pending)} />);

    expect(
      screen.getByRole("heading", { level: 1, name: "JOCKY" }),
    ).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Checking controller");
    expect(screen.getByLabelText("Version scope")).toHaveTextContent(
      "Endpoint registration, task operations, and forensic result views are intentionally unavailable.",
    );
  });

  it("shows the validated controller identity and version when healthy", async () => {
    render(<App client={clientReturning(Promise.resolve(healthySnapshot))} />);

    expect(await screen.findByText("Online")).toHaveAttribute("role", "status");
    expect(screen.getByText("JOCKY Controller")).toBeInTheDocument();
    expect(screen.getByText("0.0.0")).toBeInTheDocument();
  });

  it("shows unavailable for a typed controller failure", async () => {
    const failure = new ControllerClientError(
      "http",
      "Controller returned HTTP 503.",
      503,
    );

    render(<App client={clientReturning(Promise.reject(failure))} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("Unavailable");
    expect(
      screen.getByText("Controller returned HTTP 503."),
    ).toBeInTheDocument();
    expect(screen.queryByText("JOCKY Controller")).not.toBeInTheDocument();
  });

  it("does not expose unexpected internal errors", async () => {
    render(
      <App
        client={clientReturning(
          Promise.reject(new Error("token=must-not-be-rendered")),
        )}
      />,
    );

    expect(await screen.findByRole("alert")).toHaveTextContent("Unavailable");
    expect(
      screen.getByText("Controller could not be reached."),
    ).toBeInTheDocument();
    expect(screen.queryByText(/must-not-be-rendered/)).not.toBeInTheDocument();
  });
});
