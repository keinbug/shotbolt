#!/usr/bin/env bash
# Publish a Shotbolt installer on the update server.
#
# usage: scripts/publish-update.sh <Shotbolt-X.Y.Z-setup.exe> [notes]
#
# Uploads the installer to <remote dir>/releases/X.Y.Z/ and then replaces
# latest.json, which installed Shotbolt builds read once a day
# (windows-ui/src/update/check.rs). latest.json has the shape of a GitHub
# release so the client parses one format:
#   { "tag_name": "vX.Y.Z", "html_url": ..., "assets": [ {name, browser_download_url, size} ] }
#
# Settings (environment):
#   SHOTBOLT_UPDATE_HOST  ssh target            (default root@5.83.154.137)
#   SHOTBOLT_UPDATE_DIR   folder on the server  (default /srv/auric-updates/shotbolt)
#   SHOTBOLT_UPDATE_URL   its public URL        (default https://auric-upload.de/shotbolt)
#   SHOTBOLT_SSH_KEY      private key file      (default: ssh's own keys)
#   SHOTBOLT_KNOWN_HOSTS  known_hosts file      (default: ssh's own)
set -euo pipefail

installer=${1:?usage: publish-update.sh <Shotbolt-X.Y.Z-setup.exe> [notes]}
notes=${2:-}
host=${SHOTBOLT_UPDATE_HOST:-root@5.83.154.137}
remote=${SHOTBOLT_UPDATE_DIR:-/srv/auric-updates/shotbolt}
public=${SHOTBOLT_UPDATE_URL:-https://auric-upload.de/shotbolt}

ssh_opts=(-o BatchMode=yes)
[[ -n ${SHOTBOLT_SSH_KEY:-} ]] && ssh_opts+=(-o IdentitiesOnly=yes -i "$SHOTBOLT_SSH_KEY")
[[ -n ${SHOTBOLT_KNOWN_HOSTS:-} ]] && ssh_opts+=(-o StrictHostKeyChecking=yes -o "UserKnownHostsFile=$SHOTBOLT_KNOWN_HOSTS")

name=$(basename "$installer")
if [[ ! $name =~ ^Shotbolt-([0-9]+(\.[0-9]+)+)-setup\.exe$ ]]; then
  echo "expected Shotbolt-<version>-setup.exe, got $name" >&2
  exit 1
fi
version=${BASH_REMATCH[1]}
size=$(wc -c < "$installer" | tr -d ' ')
sha=$(sha256sum "$installer" | cut -d' ' -f1)
published=$(date -u +%Y-%m-%dT%H:%M:%SZ)

json_escape() { python -c 'import json,sys; print(json.dumps(sys.argv[1]))' "$1"; }

staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

cat > "$staging/latest.json" <<EOF
{
  "tag_name": "v$version",
  "name": "Shotbolt $version",
  "html_url": "$public/",
  "published_at": "$published",
  "draft": false,
  "prerelease": false,
  "body": $(json_escape "$notes"),
  "assets": [
    {
      "name": "$name",
      "browser_download_url": "$public/releases/$version/$name",
      "size": $size,
      "sha256": "$sha"
    }
  ]
}
EOF

cat > "$staging/index.html" <<EOF
<!doctype html>
<html lang="de"><head><meta charset="utf-8"><title>Shotbolt</title>
<meta name="viewport" content="width=device-width,initial-scale=1">
<style>body{font:16px system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#222}
a.dl{display:inline-block;padding:.6rem 1rem;background:#5b3fd1;color:#fff;border-radius:8px;text-decoration:none}
code{font-size:.85em;word-break:break-all;color:#555}</style></head>
<body><h1>Shotbolt $version</h1>
<p>$(printf '%s' "${notes:-Neue Version.}" | sed 's/&/\&amp;/g; s/</\&lt;/g; s/>/\&gt;/g')</p>
<p><a class="dl" href="releases/$version/$name">$name herunterladen</a></p>
<p>SHA-256: <code>$sha</code></p>
<p>Veröffentlicht: $published</p></body></html>
EOF

echo "Uploading $name ($size bytes) to $host:$remote/releases/$version/"
ssh "${ssh_opts[@]}" "$host" "mkdir -p '$remote/releases/$version'"
scp "${ssh_opts[@]}" "$installer" "$host:$remote/releases/$version/$name.part"
ssh "${ssh_opts[@]}" "$host" "cd '$remote/releases/$version' && echo '$sha  $name.part' | sha256sum -c --quiet && mv '$name.part' '$name' && chmod 644 '$name'"

# latest.json last, so clients never see a version whose installer is missing.
scp "${ssh_opts[@]}" "$staging/index.html" "$host:$remote/index.html.part"
scp "${ssh_opts[@]}" "$staging/latest.json" "$host:$remote/latest.json.part"
ssh "${ssh_opts[@]}" "$host" "cd '$remote' && chmod 644 index.html.part latest.json.part && mv index.html.part index.html && mv latest.json.part latest.json"

echo "Checking $public/latest.json"
curl -fsS "$public/latest.json" | python -c 'import json,sys; d=json.load(sys.stdin); print("live:", d["tag_name"], d["assets"][0]["browser_download_url"])'
curl -fsSI "$public/releases/$version/$name" | head -1
