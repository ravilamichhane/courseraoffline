# Database migrations

Run `just migrate` to upgrade `courses.db`, or `just migrate /path/to/database.db`.
Server startup also applies pending migrations before opening the connection pool.
Use `just migrate-status` to inspect an existing database without modifying it,
`just migrate-check` to validate migrations in memory, and `just migrate-test`
to test fresh and existing database upgrades and schema constraints.

Migrations are embedded at compile time using `include_dir` and loaded with
`rusqlite_migration`. Add a consecutively numbered `NN-description/up.sql`
directory for each new migration. Never edit an already deployed migration.
The library tracks the applied version using SQLite's `user_version`; it does
not track migration checksums. Migrations are forward-only; back up the database
before applying upgrades. The migration library owns migration transactions.

The first migration creates the catalog on an empty database. Databases with
the original unversioned development table require a separate manual conversion
before these migrations can be applied.

All timestamps are Unix milliseconds. The application must update `updated_at`
and job revisions when changing records. Ordering positions are zero-based.
Upsert imports by remote identity; only mark missing content removed after a
complete successful refresh. Composite foreign keys enforce course consistency
through modules, lessons, and items.

Settings have exactly one seeded row (`id = 1`). Copy the settings into a job's
`selection_json` when planning downloads. Resolve one video representation per
item and save selected subtitles against that same item. Only one non-retired
video asset per item is allowed. For replacement, retire the previous asset and
insert a new one; keep the old local file until the replacement succeeds.
The downloader must implement quality fallback and language selection.

Resolve the configured download directory to an absolute path when creating a
local file and save it in `storage_root`. Relative paths must be validated by the
application to stay inside that root. Changing settings does not relocate files.
Local files and download history restrict asset deletion, so archive/retire
catalog entries rather than deleting downloaded content implicitly.

The worker must claim queued tasks transactionally, recover interrupted running
tasks on startup, reconcile filesystem state, and publish updates after commits.
The schema prevents simultaneous running transfers of the same asset. It does
not implement a scheduler, downloader, playback service, or streaming transport.

Connections enable foreign keys and a five-second busy timeout. Migration setup
enables WAL. Keep network and filesystem work outside database transactions, and
run synchronous database operations off async request executor threads.
