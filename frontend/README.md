# Courseviewer

React, Mantine, TanStack Router, and a Rust ConnectRPC backend with SQLite.

## Run

From the repository root:

```sh
cargo run
```

In another terminal:

```sh
cd frontend
pnpm install
pnpm dev
```

Open http://localhost:3000/admin. The frontend proxies ConnectRPC requests to localhost:8080. `/` redirects to `/admin` until the learner library is implemented.

## Catalog and downloads

- Import a course with its Coursera `/learn/{slug}` slug.
- Import a specialisation with its `/specializations/{slug}` slug. Coursera supplies its details and ordered course list.
- Refresh imports to update metadata without creating duplicate records.
- Download a course or a whole specialisation. The server saves a job and downloads files in the background, independently of the browser.
- Settings are captured when a job is queued. Video quality/format, subtitle languages, supplements, directory, and concurrent file limits apply to that job.
- The worker saves the course outline, videos, available requested subtitles, supported readings, and directly linked attachments. It reports locked or unsupported content as errors; it does not submit assessments or bypass course access.
- Interrupted jobs recover at server startup. Completed files are reused; incomplete files restart. Cancelling a job preserves completed files.

Coursera requests use the existing `cookie.txt` in the server working directory. It must contain a valid Coursera session for protected materials. The local admin app has no authentication.

## Development

```sh
buf generate                 # Rust and TypeScript RPC bindings
buf format -w proto/admin.proto
cargo fmt
cargo test --lib             # Includes a local mock provider and real file transfers
cd frontend
pnpm format                 # Formats source files
pnpm check
pnpm exec tsc --noEmit
pnpm build
```

The RPC contract is `proto/admin.proto`. Generated files are in `src/generated/` and `frontend/src/gen/`.
