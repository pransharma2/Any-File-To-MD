"""Tiny local web server (standard library only) that powers the UI."""

from __future__ import annotations

import json
import os
import secrets
import socket
import string
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from . import converter

STATIC = Path(__file__).parent / "static"
TOKEN = secrets.token_urlsafe(24)


class Job:
    def __init__(self):
        self.progress = converter.Progress()
        self.cancel = threading.Event()
        self.thread: threading.Thread | None = None

    @property
    def running(self) -> bool:
        return bool(self.thread and self.thread.is_alive())


JOB = Job()


def list_drives() -> list[str]:
    if os.name != "nt":
        return ["/"]
    return [f"{c}:\\" for c in string.ascii_uppercase if Path(f"{c}:\\").exists()]


def quick_places() -> list[dict]:
    home = Path.home()
    places = [("Home", home)]
    for name in ("Desktop", "Documents", "Downloads"):
        p = home / name
        if p.is_dir():
            places.append((name, p))
    return [{"name": n, "path": str(p)} for n, p in places]


def browse(raw: str) -> dict:
    if not raw:
        return {"path": "", "parent": None, "dirs": [{"name": d, "path": d} for d in list_drives()],
                "places": quick_places(), "files": 0}
    p = Path(raw).expanduser()
    if not p.is_dir():
        raise ValueError("Folder not found")
    p = p.resolve()
    dirs, files = [], 0
    try:
        for e in sorted(p.iterdir(), key=lambda x: x.name.lower()):
            try:
                if e.is_dir():
                    if not e.name.startswith((".", "$")):
                        dirs.append({"name": e.name, "path": str(e)})
                elif e.is_file():
                    files += 1
            except OSError:
                continue
    except PermissionError:
        raise ValueError("No permission to open that folder")
    parent = str(p.parent) if p.parent != p else ""
    return {"path": str(p), "parent": parent, "dirs": dirs, "places": quick_places(), "files": files}


def open_in_explorer(path: str) -> None:
    if sys.platform == "win32":
        os.startfile(path)  # noqa: S606
    elif sys.platform == "darwin":
        subprocess.Popen(["open", path])
    else:
        subprocess.Popen(["xdg-open", path])


class Handler(BaseHTTPRequestHandler):
    server_version = "AnyToMD"

    def log_message(self, *args):  # keep the console quiet
        pass

    # ---- helpers -------------------------------------------------------
    def _host_ok(self) -> bool:
        # Blocks DNS-rebinding: only answer requests addressed to localhost.
        host = (self.headers.get("Host") or "").rsplit(":", 1)[0].strip("[]")
        return host in {"127.0.0.1", "localhost", "::1"}

    def _authed(self) -> bool:
        return self._host_ok() and secrets.compare_digest(self.headers.get("X-Token", ""), TOKEN)

    def _send(self, code: int, body: bytes, ctype: str = "application/json") -> None:
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _json(self, data, code: int = 200) -> None:
        self._send(code, json.dumps(data).encode())

    def _body(self) -> dict:
        n = int(self.headers.get("Content-Length") or 0)
        return json.loads(self.rfile.read(n) or b"{}")

    # ---- routes --------------------------------------------------------
    def do_GET(self):
        url = urlparse(self.path)
        if url.path == "/":
            if not self._host_ok():
                return self._send(403, b"Forbidden", "text/plain")
            html = (STATIC / "index.html").read_text(encoding="utf-8").replace("__TOKEN__", TOKEN)
            return self._send(200, html.encode(), "text/html; charset=utf-8")
        if url.path == "/favicon.svg":
            return self._send(200, (STATIC / "favicon.svg").read_bytes(), "image/svg+xml")
        if url.path.startswith("/fonts/"):
            name = url.path.rsplit("/", 1)[-1]
            font = STATIC / "fonts" / name
            if name.endswith(".woff2") and "/" not in name and "\\" not in name and font.is_file():
                return self._send(200, font.read_bytes(), "font/woff2")
            return self._send(404, b"Not found", "text/plain")
        if not self._authed():
            return self._json({"error": "Unauthorized"}, 403)
        if url.path == "/api/browse":
            try:
                return self._json(browse(parse_qs(url.query).get("path", [""])[0]))
            except ValueError as e:
                return self._json({"error": str(e)}, 400)
        if url.path == "/api/status":
            return self._json(JOB.progress.snapshot())
        self._json({"error": "Not found"}, 404)

    def do_POST(self):
        if not self._authed():
            return self._json({"error": "Unauthorized"}, 403)
        path = urlparse(self.path).path
        try:
            data = self._body()
        except (ValueError, json.JSONDecodeError):
            return self._json({"error": "Bad request"}, 400)

        if path == "/api/suggest":
            src = Path(str(data.get("source", "")).strip().strip('"'))
            return self._json({"output": str(converter.default_output(src))})

        if path == "/api/start":
            if JOB.running:
                return self._json({"error": "A conversion is already running."}, 409)
            src = Path(str(data.get("source", "")).strip().strip('"'))
            out_raw = str(data.get("output", "")).strip().strip('"')
            out = Path(out_raw) if out_raw else converter.default_output(src)
            err = converter.validate(src, out)
            if err:
                return self._json({"error": err}, 400)
            JOB.progress, JOB.cancel = converter.Progress(state="scanning"), threading.Event()
            JOB.thread = threading.Thread(
                target=converter.convert_folder, args=(src, out, JOB.progress, JOB.cancel), daemon=True
            )
            JOB.thread.start()
            return self._json({"ok": True, "output": str(out)})

        if path == "/api/cancel":
            JOB.cancel.set()
            return self._json({"ok": True})

        if path == "/api/open":
            target = JOB.progress.output
            if target and Path(target).is_dir():
                open_in_explorer(target)
                return self._json({"ok": True})
            return self._json({"error": "Nothing to open yet."}, 400)

        self._json({"error": "Not found"}, 404)


def find_port(preferred: int = 8765) -> int:
    for port in range(preferred, preferred + 50):
        with socket.socket() as s:
            if s.connect_ex(("127.0.0.1", port)) != 0:
                return port
    raise RuntimeError("No free port found")


def make_server(port: int | None = None) -> ThreadingHTTPServer:
    return ThreadingHTTPServer(("127.0.0.1", port or find_port()), Handler)
