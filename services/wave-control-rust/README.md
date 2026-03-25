# `rust-wave-control`

Rust HTTP service for the hosted Wave control plane mirror.

Current responsibilities:

- serve a health endpoint for Railway
- build a standalone binary for Railway startup from the workspace crate
- expose live repo reads for operator snapshots, control events, coordination records, and result envelopes when `WAVE_REPO_ROOT` points at a local repo
- accept and return mirrored operator snapshots as JSON
- accept and return mirrored control events, coordination records, and result envelopes per wave
- store mirrored data in Postgres when `DATABASE_URL` is set, otherwise fall back to file-backed JSON under `WAVE_CONTROL_DATA_DIR`

Environment:

- `PORT`: listener port, defaults to `3000`
- `HOST`: listener host, defaults to `0.0.0.0`
- `DATABASE_URL`: optional Postgres mirror storage; when present the service creates additive `rust_wave_*` tables
- `WAVE_CONTROL_API_TOKEN`: optional bearer token required for `/v1/*` endpoints when set
- `WAVE_CONTROL_DATA_DIR`: mirror storage root, defaults to `./data`
- `WAVE_REPO_ROOT`: optional repo root for live snapshot and live control-event reads
- `WAVE_CONFIG_PATH`: optional config path, defaults to `<WAVE_REPO_ROOT>/wave.toml`
- `WAVE_CONTROL_PROJECT_KEY`: default project key for mirror reads/writes, defaults to `default`
- `WAVE_CONTROL_ORCHESTRATOR_ID`: stored mirror source label, defaults to `rust-wave`
- `WAVE_CONTROL_RUNTIME_VERSION`: stored runtime version label, defaults to the crate version

Mirror requests can override the default project key with the `X-Wave-Project` header.

Endpoints:

- `GET /healthz`
- `GET /v1/operator/snapshot`
- `POST /v1/operator/snapshot`
- `GET /v1/control-events/:wave_id`
- `POST /v1/control-events/:wave_id`
- `GET /v1/coordination/:wave_id`
- `POST /v1/coordination/:wave_id`
- `GET /v1/result-envelopes/:wave_id`
- `POST /v1/result-envelopes/:wave_id`
