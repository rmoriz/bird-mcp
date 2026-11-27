#!/bin/sh
# Reuse the host's OpenCode Go login in the lab container.
#
# Reads the active `opencode-go` API key from the host's OpenCode database and
# writes it to tests/compose/.env as OPENCODE_API_KEY (gitignored, mode 600).
# Compose passes it into the container, so no second login is needed.
set -eu

cd "$(dirname "$0")"

host_data="${OPENCODE_DATA_DIR:-$HOME/.local/share/opencode}"
host_db="$host_data/opencode.db"
if [ ! -f "$host_db" ]; then
  echo "No OpenCode database at $host_db" >&2
  echo "Run 'opencode auth login' on the host first, then retry." >&2
  exit 1
fi

key=$(sqlite3 "$host_db" \
  "select json_extract(value,'\$.key') from credential
   where integration_id = 'opencode-go' and active = 1
   order by time_updated desc limit 1;")

if [ -z "$key" ] || [ "$key" = "null" ]; then
  echo "No active OpenCode Go credential in $host_db." >&2
  echo "Connect it with 'opencode auth login' on the host first." >&2
  exit 1
fi

umask 077
if [ -f .env ]; then
  grep -v '^OPENCODE_API_KEY=' .env >.env.tmp || true
  mv .env.tmp .env
fi
printf 'OPENCODE_API_KEY=%s\n' "$key" >>.env
chmod 600 .env

echo "Wrote OPENCODE_API_KEY from the host login to tests/compose/.env"
docker compose up -d --no-deps --force-recreate opencode
docker compose exec -T opencode opencode auth list
