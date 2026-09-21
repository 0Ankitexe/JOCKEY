import { useEffect, useState } from "react";

import {
  ControllerClientError,
  defaultControllerClient,
  describeControllerError,
  type ControllerClient,
  type ControllerSnapshot,
} from "./api/controller";
import "./App.css";

type ControllerState =
  | { readonly kind: "loading" }
  | { readonly kind: "ready"; readonly snapshot: ControllerSnapshot }
  | { readonly kind: "error"; readonly message: string };

interface AppProps {
  readonly client?: ControllerClient;
}

function statusErrorMessage(error: unknown): string {
  if (error instanceof ControllerClientError) {
    return describeControllerError(error);
  }
  return "Controller could not be reached.";
}

function ControllerStatus({ state }: { readonly state: ControllerState }) {
  if (state.kind === "loading") {
    return (
      <div className="controller-result controller-result--loading">
        <span className="status-dot" aria-hidden="true" />
        <p role="status">Checking controller…</p>
      </div>
    );
  }

  if (state.kind === "error") {
    return (
      <div className="controller-result controller-result--error">
        <span className="status-dot" aria-hidden="true" />
        <div>
          <p className="status-label" role="alert">
            Unavailable
          </p>
          <p className="status-detail">{state.message}</p>
        </div>
      </div>
    );
  }

  return (
    <div className="controller-result controller-result--ready">
      <span className="status-dot" aria-hidden="true" />
      <div>
        <p className="status-label" role="status">
          Online
        </p>
        <dl className="version-details">
          <div>
            <dt>Service</dt>
            <dd>{state.snapshot.version.name}</dd>
          </div>
          <div>
            <dt>Version</dt>
            <dd>{state.snapshot.version.version}</dd>
          </div>
        </dl>
      </div>
    </div>
  );
}

export function App({ client = defaultControllerClient }: AppProps) {
  const [controllerState, setControllerState] = useState<ControllerState>({
    kind: "loading",
  });

  useEffect(() => {
    const abortController = new AbortController();

    void client
      .getStatus(abortController.signal)
      .then((snapshot) => {
        if (!abortController.signal.aborted) {
          setControllerState({ kind: "ready", snapshot });
        }
      })
      .catch((error: unknown) => {
        if (!abortController.signal.aborted) {
          setControllerState({
            kind: "error",
            message: statusErrorMessage(error),
          });
        }
      });

    return () => {
      abortController.abort();
    };
  }, [client]);

  return (
    <main className="page-shell">
      <header className="hero">
        <p className="eyebrow">Authorised forensic investigation framework</p>
        <h1>JOCKY</h1>
        <p className="summary">
          Foundation services are configured for isolated university lab use.
        </p>
      </header>

      <section className="status-grid" aria-label="System status">
        <article className="status-card">
          <p className="card-label">Dashboard</p>
          <div className="local-status">
            <span className="status-dot" aria-hidden="true" />
            <p className="status-label">Ready</p>
          </div>
          <p className="status-detail">Version 0 status interface</p>
        </article>

        <article className="status-card">
          <p className="card-label">Controller</p>
          <ControllerStatus state={controllerState} />
        </article>
      </section>

      <aside className="scope-note" aria-label="Version scope">
        <strong>Version 0 scope</strong>
        <p>
          This page reports service health only. Endpoint registration, task
          operations, and forensic result views are intentionally unavailable.
        </p>
      </aside>
    </main>
  );
}
