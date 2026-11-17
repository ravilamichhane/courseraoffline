# CourseViewer

CourseViewer is a self-hosted web application and offline media server for importing, downloading, organizing, and studying Coursera courses and specializations locally.

It pairs a Rust/Actix Web backend (with ConnectRPC and SQLite) and a React/Mantine frontend into an offline-capable library and course player—available either as a single compiled binary with embedded assets or via Docker.

---

## Features

- **Catalog & Outline Import**:
  - Import individual Coursera courses (`/learn/{slug}`) and specializations (`/specializations/{slug}`) by slug.
  - Preserves the full module → lesson → item hierarchy and refreshes catalog metadata without creating duplicate records.
- **Persistent Background Download Manager**:
  - Queues course or specialization downloads independently of the browser and persists job/task state in SQLite.
  - Captures download preferences per job: preferred video resolution (`240p`, `360p`, `540p`, `720p`, `1080p`), format (`mp4`, `webm`), and quality fallback policy (`best_at_or_below` or `exact`).
  - Downloads subtitle tracks (`.vtt`, `.srt`, `.txt` in priority order) across configured languages, HTML reading supplements, and linked attachments.
  - Supports configurable concurrency (`1`–`32` concurrent transfers), pause, resume, cancel, and retry (reusing completed files), plus automatic startup recovery for interrupted tasks.
- **Learner Library & Course Player**:
  - **Library (`/`)**: Search and filter across `All learning`, `Specialisations`, `Courses`, and `Downloaded` content, with overall and per-course completion progress.
  - **Course Viewer (`/courses/$courseSlug`)**: Collapsible module/lesson/item outline with outline search, HTTP `Range`-enabled local media streaming (`/local-media/*`), attached HTML5 subtitle tracks, playback speed controls (`0.25x`–`3x` with `[` / `]`, `<` / `>`, and `\` reset shortcuts, plus `±10s` seek), auto-play next item, KaTeX math rendering inside HTML readings, downloadable attachments, and automatic/manual item completion tracking.
- **Admin Console (`/admin`)**:
  - **Overview (`/admin`)**: Storage usage, catalog summary, and active job overview.
  - **Courses (`/admin/courses`)** & **Specialisations (`/admin/specializations`)**: Import, refresh, archive/unarchive, queue downloads, and delete entries.
  - **Downloads (`/admin/downloads`)**: Inspect job and per-course/module/lesson/item/asset progress and control running or paused jobs.
  - **Configuration (`/admin/settings`)**: Manage Coursera session cookies (paste or upload Netscape `cookies.txt`, Cookie-Editor JSON, or raw `Cookie` header), video quality/format preferences, subtitle languages, supplementary resource downloads, and concurrency limits.
- **Single-Binary & Docker Deployment**:
  - SQLite migrations (`migrations/`) and built frontend assets (`frontend/dist`) are embedded into the Rust binary at compile time via `include_dir`.
  - In development mode, the backend proxies frontend requests to the Vite dev server for hot reloading.

---

## Tech Stack

- **Backend**: Rust (Edition 2024), Actix Web 4, ConnectRPC (`connectrpc` + `buffa`), SQLite (`rusqlite` bundled, `r2d2`, `r2d2_sqlite`, `rusqlite_migration`), Tokio, Reqwest
- **Frontend**: React 19, TypeScript, Vite 8, TanStack Router, ConnectRPC (`@connectrpc/connect`, `@connectrpc/connect-web`, `@connectrpc/connect-query`), Mantine v9, Tailwind CSS v4, Phosphor Icons, KaTeX, Biome
- **Schema & Codegen**: Protocol Buffers (`proto/admin.proto`) with Buf (`buf.yaml`, `buf.gen.yaml`)

---

## Project Structure

```text
.
├── proto/
│   └── admin.proto                  # ConnectRPC AdminService & message definitions
├── migrations/                      # Embedded forward-only SQLite migrations (01–06)
├── src/
│   ├── main.rs                      # HTTP server entrypoint & configuration wiring
│   ├── lib.rs                       # Env loading & data/db/cookie path helpers
│   ├── auth.rs                      # API key extraction & /api/auth/verify endpoint
│   ├── admin.rs                     # ConnectRPC AdminService handlers & router adapter
│   ├── downloads.rs                 # Background download planner, worker & recovery
│   ├── coursera.rs                  # Coursera catalog & outline import helpers
│   ├── courseeraclient.rs           # Coursera HTTP client & cookie parser
│   ├── courseeraresponses.rs        # Coursera API response models
│   ├── courses.rs                   # Authenticated /courses/* proxy endpoints
│   ├── database.rs                  # SQLite pool setup & migration runner
│   ├── frontend.rs                  # /local-media Range streaming & frontend serving/proxy
│   ├── bin/migrate.rs               # Standalone database migration CLI
│   └── generated/                   # Buf-generated Rust protobuf & ConnectRPC bindings
├── frontend/
│   ├── src/
│   │   ├── routes/                  # TanStack Router pages (/, /courses/$courseSlug, /admin/*)
│   │   ├── admin/                   # Admin API client, auth context & shared UI
│   │   ├── learner/                 # Progress & KaTeX math rendering helpers
│   │   └── gen/                     # Buf-generated TypeScript protobuf & Connect clients
│   ├── package.json
│   └── vite.config.ts               # Vite dev server & backend proxy rules
├── data/                            # Default runtime data dir (courses.db, downloads/, cookie.txt)
├── .env.example                     # Sample environment configuration
├── buf.yaml / buf.gen.yaml          # Buf lint & codegen configuration
├── build.rs                         # Ensures frontend/dist exists & watches migrations
├── Cargo.toml
├── Dockerfile                       # Multi-stage production container build
└── justfile                         # Task runner shortcuts
```

---

## Getting Started

### Prerequisites

- **Rust toolchain** (supporting Rust Edition 2024)
- **Node.js 22+** and **pnpm**
- Optional: [`just`](https://just.systems/) (command runner) and [`buf`](https://buf.build/) (for modifying `proto/admin.proto`)

### 1. Environment Setup

Copy the example environment file and set your `API_KEY`:

```sh
cp .env.example .env
```

Default `.env.example` values:

```dotenv
MODE=development
API_KEY=dev
```

> **Note:** `API_KEY` is required—the server will refuse to start if it is unset or empty.

### 2. Running in Development

Install frontend dependencies first:

```sh
cd frontend && pnpm install
```

Start the Vite dev server (port `3000`) and the Rust backend (port `8080`) in two terminals:

```sh
# Terminal 1 — Frontend dev server (http://localhost:3000)
just frontend
# or: cd frontend && pnpm run dev

# Terminal 2 — Rust backend (http://localhost:8080)
just backend
# or: MODE=development API_KEY=dev cargo run
```

In development mode (`MODE=development`):
- Accessing `http://localhost:8080` proxies frontend requests to Vite (`http://localhost:3000` by default).
- Accessing `http://localhost:3000` directly proxies `/courseviewer.admin.v1.AdminService`, `/api`, and `/local-media` requests to the backend on `http://127.0.0.1:8080`.

### 3. Building for Production

Build the frontend bundle first so `include_dir` embeds the latest assets into the Rust binary, then build the release binaries:

```sh
just frontend-build
# or: cd frontend && pnpm run build

cargo build --release --bin courseviewer --bin migrate
```

Run the compiled server:

```sh
API_KEY=your-secret-key ./target/release/courseviewer
```

When `MODE` is omitted or set to `production`, the server serves assets from `frontend/dist` on disk if present, falling back to the `frontend/dist` snapshot embedded at compile time.

---

## Docker Deployment

The included multi-stage [`Dockerfile`](Dockerfile) builds the frontend, compiles the `courseviewer` and `migrate` release binaries, and runs on `debian:bookworm-slim` with `MODE=production`, `DATA_DIR=/data`, and port `8080` exposed.

Build and run the container:

```sh
docker build -t courseviewer .

docker run -d \
  --name courseviewer \
  -p 8080:8080 \
  -e API_KEY=your-secret-key \
  -v "$(pwd)/data:/data" \
  courseviewer
```

---

## Configuration & Authentication

### Environment Variables

The backend reads variables from the process environment first, then falls back to `.env` in the working directory:

| Variable | Required | Default | Description |
| --- | --- | --- | --- |
| `API_KEY` | Yes | — | Secret key required to authenticate API, admin RPC, and local media requests. |
| `MODE` (or `APP_ENV`) | No | `production` | Set to `development` or `dev` to proxy frontend requests to the Vite dev server; otherwise serves `frontend/dist` / embedded assets. |
| `DATA_DIR` (or `DATA_FOLDER`) | No | `./data` | Directory where `courses.db`, `downloads/`, and `cookie.txt` are stored. |
| `HOST` | No | `0.0.0.0` | Network interface to bind. |
| `PORT` | No | `8080` | Port to listen on. |
| `FRONTEND_DEV_URL` | No | `http://localhost:3000` | Upstream Vite URL used when `MODE=development`. |

### Data Directory Layout

By default (`DATA_DIR=./data`), runtime state lives under:

- `data/courses.db` — SQLite catalog, download jobs/tasks, playback completion state, and application settings.
- `data/downloads/` — Downloaded course videos, subtitles, HTML readings, and attachments.
- `data/cookie.txt` — Persisted Coursera session cookie string.

### API Key Authentication

All ConnectRPC endpoints (`/courseviewer.admin.v1.AdminService/*`), local media routes (`/local-media/*`), and `/courses/*` routes require `API_KEY`. Authentication can be supplied via any of:

- `X-API-Key: <API_KEY>` header
- `Authorization: Bearer <API_KEY>` header
- `courseviewer_api_key` or `api_key` cookie
- `?api_key=<API_KEY>` query parameter

The `/api/auth/verify` endpoint checks whether a request is authorized. In the web UI, entering the API key stores it in `localStorage` and syncs the `courseviewer_api_key` cookie so native `<video>` and `<track>` elements can stream `/local-media/*` seamlessly.

### Coursera Session Cookies

Importing protected courses and downloading course assets requires a valid Coursera session cookie (including `CAUTH`, and optionally `CSRF3-Token`). You can configure this in either of two ways:

1. **Admin UI**: Open `/admin/settings` (**Configuration**) and paste or upload your cookies in Netscape `cookies.txt` format, Cookie-Editor JSON export format, or a raw `Cookie` header (`CAUTH=...; CSRF3-Token=...`). Saving updates the live client and persists to `data/cookie.txt`.
2. **File**: Place your cookie file directly at `<DATA_DIR>/cookie.txt` before starting the server.

---

## Database Migrations & Testing

SQLite migrations live in `migrations/` (`01-catalog` through `06-application-settings`) and are applied automatically on server startup. See [`migrations/README.md`](migrations/README.md) for schema conventions.

### Migration CLI (`just` or `cargo`)

```sh
just migrate                       # Apply pending migrations to data/courses.db
just migrate /path/to/courses.db   # Apply pending migrations to a custom path
just migrate-status                # Print current SQLite user_version
just migrate-check                 # Validate all embedded migrations in memory
just migrate-test                  # Run database & migration tests
```

Equivalent `cargo` commands:

```sh
cargo run --bin migrate -- up data/courses.db
cargo run --bin migrate -- status data/courses.db
cargo run --bin migrate -- check
```

### Running Tests & Checks

```sh
# Backend tests (includes SQLite migration tests, API auth tests, and mock provider download tests)
cargo test

# Frontend lint, formatting, and type checks
cd frontend
pnpm check
pnpm exec tsc --noEmit
pnpm format
```

---

## Protobuf Code Generation

The `AdminService` contract is defined in [`proto/admin.proto`](proto/admin.proto) and configured in [`buf.gen.yaml`](buf.gen.yaml). Generated code is checked into `src/generated/` (Rust `buffa` + `connectrpc`) and `frontend/src/gen/` (TypeScript `protoc-gen-es` + `protoc-gen-connect-query`).

After editing `proto/admin.proto`:

```sh
buf lint
buf format -w proto/admin.proto
buf generate
```
