#!/usr/bin/env bash
# Runs the public Postgres API through the installed AOT VM and external worker.
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
terlc="${TERLC:-${repo_root}/target/debug/terlc}"
fixture_name="terlan-postgres-source-$$-${RANDOM}"
trap 'docker rm --force "${fixture_name}" >/dev/null 2>&1 || true' EXIT

test -x "${terlc}"
test -x "$(dirname "${terlc}")/terlan-native-worker"
docker run --detach --rm --name "${fixture_name}" \
  --env POSTGRES_USER=terlan --env POSTGRES_PASSWORD=terlan \
  --env POSTGRES_DB=terlan --publish 127.0.0.1::5432 \
  postgres:16-alpine >/dev/null
ready=false
for _ in {1..100}; do
  if docker exec "${fixture_name}" pg_isready --host 127.0.0.1 --username terlan --dbname terlan >/dev/null 2>&1; then
    ready=true
    break
  fi
  sleep 0.1
done
if [[ "${ready}" != true ]]; then
  echo 'Postgres source fixture did not become ready' >&2
  exit 1
fi
address="$(docker port "${fixture_name}" 5432/tcp)"
port="${address##*:}"
[[ "${port}" =~ ^[0-9]+$ ]]
export TERLAN_TEST_POSTGRES_URL="postgres://terlan:terlan@127.0.0.1:${port}/terlan"
cd "${repo_root}"
"${terlc}" test tests/fixtures/postgres_worker/PostgresWorkerTest.terl
