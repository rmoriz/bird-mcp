#!/bin/sh
# Copy the OpenCode Go credential from the host's OpenCode database into the
# lab's opencode_auth volume. Only the `opencode-go` rows are copied, so the
# container can use the host subscription without a second login. The secret
# stays in a temporary directory and is never printed.
set -eu

cd "$(dirname "$0")"

if [ -n "${OPENCODE_DATA_DIR:-}" ]; then
  echo "OPENCODE_DATA_DIR is set; the container already mounts the host data directory."
  exit 0
fi

host_db="${HOME}/.local/share/opencode/opencode.db"
if [ ! -f "$host_db" ]; then
  echo "No OpenCode database at $host_db" >&2
  echo "Run 'opencode auth login' on the host first, then retry." >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
umask 077

sqlite3 "$host_db" ".mode insert credential" \
  "select id, integration_id, label, value, connector_id, method_id, active, time_created, time_updated
   from credential where integration_id = 'opencode-go';" >"$tmp/opencode-go.sql"

if ! grep -q "INSERT INTO" "$tmp/opencode-go.sql"; then
  echo "No OpenCode Go credential found in $host_db." >&2
  echo "Connect it with 'opencode auth login' on the host first." >&2
  exit 1
fi

cat >"$tmp/import.sh" <<'EOS'
#!/bin/sh
set -eu
db=/root/.local/share/opencode/opencode.db
mkdir -p /root/.local/share/opencode
opencode auth list >/dev/null 2>&1 || true
[ -f "$db" ] || { echo "OpenCode did not create $db" >&2; exit 1; }
sqlite3 "$db" "delete from credential where integration_id = 'opencode-go';"
sqlite3 "$db" < /import/opencode-go.sql
echo "Credentials now in the container:"
opencode auth list
EOS
chmod +x "$tmp/import.sh"

docker compose run --rm --no-deps \
  -v "$tmp:/import:ro" \
  --entrypoint /import/import.sh \
  opencode
