#!/usr/bin/env python3
"""Minimal upload API for trying out Shotbolt's "Own upload API" destination.

Run:    python scripts/shotbolt-upload-test-server.py [--port 8765] [--token secret]
Then in Shotbolt → Settings → Cloud → Own upload API:
    Upload URL:        http://127.0.0.1:8765/upload
    API token:         the --token value (or empty)
    Token header:      Authorization
    File field name:   file
    Link in response:  url

Uploaded files land in ./shotbolt-uploads and are served back at
http://127.0.0.1:<port>/files/<name>. For local testing only: no TLS, no
limits, listens on 127.0.0.1.
"""

import argparse
import email.parser
import email.policy
import json
import os
import re
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

UPLOAD_DIR = os.path.abspath("shotbolt-uploads")


def parse_multipart(content_type, body):
    """Return {field_name: (filename, bytes)} for a multipart/form-data body."""
    message = email.parser.BytesParser(policy=email.policy.HTTP).parsebytes(
        b"Content-Type: " + content_type.encode() + b"\r\n\r\n" + body
    )
    fields = {}
    for part in message.iter_parts():
        name = part.get_param("name", header="content-disposition")
        if name:
            fields[name] = (part.get_filename(), part.get_payload(decode=True) or b"")
    return fields


class Handler(BaseHTTPRequestHandler):
    token = ""
    field = "file"

    def _send(self, status, payload, content_type="application/json"):
        data = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_POST(self):
        if self.path.rstrip("/") != "/upload":
            return self._send(404, {"error": "not found"})
        if self.token and self.headers.get("Authorization") != f"Bearer {self.token}":
            return self._send(401, {"error": "bad token"})
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length)
        fields = parse_multipart(self.headers.get("Content-Type", ""), body)
        if self.field not in fields:
            return self._send(400, {"error": f"missing field '{self.field}'"})
        filename, data = fields[self.field]
        safe = re.sub(r"[^A-Za-z0-9._-]", "_", filename or "upload")
        stored = f"{uuid.uuid4().hex[:8]}-{safe}"
        os.makedirs(UPLOAD_DIR, exist_ok=True)
        with open(os.path.join(UPLOAD_DIR, stored), "wb") as handle:
            handle.write(data)
        host = self.headers.get("Host", f"127.0.0.1:{self.server.server_port}")
        url = f"http://{host}/files/{stored}"
        print(f"stored {len(data)} bytes as {stored}")
        return self._send(200, {"url": url})

    def do_GET(self):
        if not self.path.startswith("/files/"):
            return self._send(404, {"error": "not found"})
        name = os.path.basename(self.path[len("/files/"):])
        path = os.path.join(UPLOAD_DIR, name)
        if not os.path.isfile(path):
            return self._send(404, {"error": "not found"})
        with open(path, "rb") as handle:
            data = handle.read()
        kind = "image/png" if name.lower().endswith(".png") else "application/octet-stream"
        return self._send(200, data, kind)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--port", type=int, default=8765)
    parser.add_argument("--token", default="")
    parser.add_argument("--field", default="file")
    args = parser.parse_args()
    Handler.token = args.token
    Handler.field = args.field
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print(f"Shotbolt test upload API on http://127.0.0.1:{args.port}/upload")
    print(f"Files go to {UPLOAD_DIR}")
    server.serve_forever()


if __name__ == "__main__":
    main()
