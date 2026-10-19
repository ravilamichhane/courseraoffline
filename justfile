# https://just.systems
set dotenv-load := true

frontend:
    cd frontend && pnpm run dev

frontend-build:
    cd frontend && pnpm run build

backend:
    MODE="${MODE:-development}" API_KEY="${API_KEY:-dev}" cargo run

# Apply pending migrations (also run automatically at server startup).
migrate database="data/courses.db":
    cargo run --bin migrate -- up "{{database}}"

migrate-status database="data/courses.db":
    cargo run --bin migrate -- status "{{database}}"

migrate-check:
    cargo run --bin migrate -- check

migrate-test:
    cargo test --lib database::tests
