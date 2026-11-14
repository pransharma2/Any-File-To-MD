"""Entry point: starts the local server and opens your browser."""

from __future__ import annotations

import argparse
import threading
import webbrowser

from . import __version__
from .server import make_server


def main() -> None:
    ap = argparse.ArgumentParser(prog="anytomd", description="Convert a folder of files to Markdown.")
    ap.add_argument("--port", type=int, default=None)
    ap.add_argument("--no-browser", action="store_true", help="don't open a browser window")
    args = ap.parse_args()

    server = make_server(args.port)
    url = f"http://127.0.0.1:{server.server_address[1]}/"
    print(f"\n  Any File To MD v{__version__}")
    print(f"  Running at {url}")
    print("  Leave this window open while you use the app. Press Ctrl+C or close it to quit.\n")
    if not args.no_browser:
        threading.Timer(0.6, webbrowser.open, args=(url,)).start()
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\n  Bye!")
    finally:
        server.server_close()


if __name__ == "__main__":
    main()
