# Shotbolt

Shotbolt is a personal Windows fork of [ApexShot](https://github.com/apex-shot/apexshot):
screenshots, screen recordings, annotation, OCR and video edits, with uploads
to **your own HTTP API**.

Shotbolt is free software under the **GPL-3.0-or-later**, like ApexShot. All
credit for the original program goes to the ApexShot authors. Their README is
kept as [README.apexshot.md](README.apexshot.md), and the licence is in
[LICENSE](LICENSE).

## What is different from ApexShot

- **Name and logo:** the app shows up as Shotbolt.
- **Own upload API:** Settings → Cloud → *Own upload API* (described below).
- **Own updates:** Shotbolt checks its own update server
  (`https://auric-upload.de/shotbolt/latest.json`), not ApexShot's.
- **Not included:** ApexShot Cloud login, the cloud and Studio hints, and the
  usage and download telemetry.
- **Studio stays locked:** this build has no Studio licence key, so Studio
  features stay locked.

Internal names stay as they were, so nothing breaks:

- program files `apexshot-*.exe`
- settings folder `%APPDATA%\apexshot`
- environment variables `APEXSHOT_*`

## Install (Windows)

1. Download `Shotbolt-<version>-setup.exe` from
   <https://auric-upload.de/shotbolt/>.
2. Run it. It installs for your user under `%LOCALAPPDATA%\Programs\Shotbolt`,
   with no administrator rights needed.

The installer is not code-signed, so SmartScreen may warn you: choose
**More info → Run anyway**.

## Upload to your own API

Settings → **Cloud** → *Upload with* → **Own upload API**:

| Field | Default | Meaning |
|---|---|---|
| Upload URL | – | Endpoint that receives the file as a `multipart/form-data` POST |
| API token | empty | Optional secret |
| Token header | `Authorization` | `Authorization` sends `Bearer <token>`. Any other header (e.g. `X-API-Key`) sends the token unchanged |
| File field name | `file` | Multipart field that holds the file |
| Link in response | `url` | Dotted JSON path to the share link, e.g. `data.link` or `files.0.url` |

The response can be JSON (the link sits at the given path) or plain text (the
body is the link itself). Shotbolt copies the link to the clipboard. With
*Upload after capture* switched on, every screenshot and recording is uploaded
automatically. **Test connection** sends a small text file and shows the link
that comes back.

A minimal compatible server for trying it out locally is
[scripts/shotbolt-upload-test-server.py](scripts/shotbolt-upload-test-server.py).

## Releasing a new version

1. Raise `version` in `Cargo.toml` and `windows-ui/Cargo.toml` (e.g. `1.0.2`),
   and in the package entries of `Cargo.lock` and `windows-ui/Cargo.lock`.
2. Commit and push, then push a tag:
   `git tag -a v1.0.2 -m "What changed" && git push origin v1.0.2`.
3. The workflow *Shotbolt release* builds and smoke-tests the installer,
   uploads it to the update server (the tag message becomes the notes) and
   attaches it to a GitHub release. Installed Shotbolt builds offer the update
   within a day, or at once through Settings → About → *Check for Updates*.

The upload uses the repository secrets `SHOTBOLT_DEPLOY_KEY` (SSH key) and
`SHOTBOLT_DEPLOY_KNOWN_HOSTS`. By hand, the same upload is
`bash scripts/publish-update.sh Shotbolt-1.0.2-setup.exe "What changed"`.
