# JOCKY dashboard

Version 0 provides a read-only status page for the local JOCKY controller. It
does not include endpoint registration, task operations, or forensic result
views.

## Local checks

Requires Node.js 20.19 or newer.

```sh
npm install
cp .env.example .env.local
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build
```

`VITE_CONTROLLER_URL` selects the controller base URL and defaults to
`http://localhost:8000`. Variables prefixed with `VITE_` are visible to the
browser and must never contain credentials or secrets.
