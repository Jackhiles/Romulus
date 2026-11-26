#!/usr/bin/env python3
"""A small fake RomM server for developing Romulus without a real one.

Implements only what Romulus calls, with response shapes from the RomM 5.4.1 OpenAPI
schema (docs/reference). Device pairing auto-approves after two polls; any password works.

    python3 tools/mock-romm/server.py --port 8099 --games 1500

Then use http://localhost:8099 as the server address.
"""

import argparse
import json
import random
import struct
import zlib
from datetime import datetime, timedelta, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

PLATFORMS = [
    ("snes", "Super Nintendo Entertainment System"),
    ("gba", "Game Boy Advance"),
    ("psx", "PlayStation"),
    ("n64", "Nintendo 64"),
    ("genesis", "Sega Mega Drive / Genesis"),
    ("nes", "Nintendo Entertainment System"),
    ("gb", "Game Boy"),
    ("ps2", "PlayStation 2"),
]
WORDS = (
    "Super Mega Dragon Quest Star Fighter Legend Zero Dark Crystal Shadow Metal Turbo Blast "
    "Kingdom Hearts Racing Rally Island Saga Chronicles Warrior Ninja Galaxy Final Fantasy "
    "Castle Night Storm Blade Soul Rogue Hero Puzzle Tactics Arena Ocean Sky Rocket"
).split()

BASE_TIME = datetime(2026, 11, 1, tzinfo=timezone.utc)


def iso(t):
    return t.strftime("%Y-%m-%dT%H:%M:%S")


def png(w, h, seed):
    """A gradient PNG, so covers look like covers."""
    rnd = random.Random(seed)
    a = [rnd.randint(30, 220) for _ in range(3)]
    b = [rnd.randint(30, 220) for _ in range(3)]
    rows = bytearray()
    for y in range(h):
        rows.append(0)
        t = y / max(h - 1, 1)
        px = bytes(int(a[i] * (1 - t) + b[i] * t) for i in range(3))
        rows.extend(px * w)

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(rows), 6))
        + chunk(b"IEND", b"")
    )


def build(n_games, seed=1):
    rnd = random.Random(seed)
    platforms = []
    for i, (slug, name) in enumerate(PLATFORMS, start=1):
        platforms.append({
            "id": i, "slug": slug, "fs_slug": slug, "name": name, "display_name": name,
            "custom_name": None, "rom_count": 0, "url_logo": None, "firmware": [],
            "updated_at": iso(BASE_TIME),
        })
    roms = []
    for rid in range(1, n_games + 1):
        p = platforms[rnd.randrange(len(platforms))]
        title = " ".join(rnd.sample(WORDS, rnd.randint(2, 4)))
        if rnd.random() < 0.15:
            title = "The " + title
        p["rom_count"] += 1
        roms.append({
            "id": rid, "platform_id": p["id"], "platform_slug": p["slug"], "platform_fs_slug": p["fs_slug"],
            "name": title, "name_sort_key": None, "fs_name": f"{title} (USA).zip",
            "fs_name_no_ext": f"{title} (USA)", "fs_size_bytes": rnd.randint(200_000, 700_000_000),
            "summary": f"{title} is a game in the mock library. " * 4,
            "alternative_names": [], "regions": ["USA"],
            "path_cover_small": f"/assets/romm/resources/roms/{p['id']}/{rid}/cover/small.png?ts=1",
            "path_cover_large": f"/assets/romm/resources/roms/{p['id']}/{rid}/cover/big.png?ts=1",
            "merged_screenshots": [f"/assets/romm/resources/roms/{p['id']}/{rid}/screenshots/{k}.png" for k in range(3)],
            "metadatum": {
                "genres": rnd.sample(["Action", "RPG", "Platform", "Racing", "Puzzle", "Shooter"], 2),
                "developers": [rnd.choice(["Mockware", "Pixel Forge", "Bitworks"])],
                "player_count": rnd.choice(["1", "1-2", "1-4"]),
                "first_release_date": int(datetime(rnd.randint(1985, 2005), 6, 1).timestamp() * 1000),
            },
            "ra_id": None,
            "updated_at": iso(BASE_TIME + timedelta(minutes=rid)),
        })
    return platforms, roms


class State:
    platforms = []
    roms = []
    polls = {}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass

    def send_json(self, obj, status=200):
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def authorized(self):
        if self.headers.get("Authorization", "").startswith("Bearer "):
            return True
        self.send_json({"detail": "Not authenticated"}, 401)
        return False

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        return self.rfile.read(n) if n else b""

    def do_POST(self):
        url = urlparse(self.path)
        if url.path == "/api/auth/device/init":
            code = f"dc{random.randrange(10**9)}"
            State.polls[code] = 0
            self.send_json({
                "device_code": code, "user_code": "MOCK-1234", "verification_path": "/pair/device",
                "verification_path_complete": "/pair/device?user_code=MOCK-1234", "expires_in": 600, "interval": 2,
            }, 201)
        elif url.path == "/api/auth/device/token":
            code = json.loads(self.body() or b"{}").get("device_code")
            if code not in State.polls:
                return self.send_json({"detail": "expired_token"}, 400)
            State.polls[code] += 1
            if State.polls[code] < 2:
                return self.send_json({"detail": "authorization_pending"}, 400)
            del State.polls[code]
            self.send_json({"access_token": "rmm_mock", "device_id": "mock-device", "scopes": [], "expires_at": None})
        elif url.path == "/api/token":
            self.body()
            self.send_json({"access_token": "mock-access", "refresh_token": "mock-refresh", "token_type": "bearer", "expires": 900})
        else:
            self.send_json({"detail": "Not Found"}, 404)

    def do_GET(self):
        url = urlparse(self.path)
        q = parse_qs(url.query)
        path = url.path
        if path == "/api/heartbeat":
            return self.send_json({"SYSTEM": {"VERSION": "5.4.1-mock", "GIT_BRANCH": None, "SHOW_SETUP_WIZARD": False}})
        if path.startswith("/assets/"):
            seed = path
            w, h = (528, 704) if "big" in path else (264, 352)
            if "screenshots" in path:
                w, h = 640, 480
            data = png(w, h, seed)
            self.send_response(200)
            self.send_header("Content-Type", "image/png")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
            return
        if not self.authorized():
            return
        after = q.get("updated_after", [None])[0]
        after_t = after.replace("Z", "") if after else None
        if path == "/api/platforms":
            items = [p for p in State.platforms if not after_t or p["updated_at"] > after_t[:19]]
            return self.send_json(items)
        if path == "/api/platforms/identifiers":
            return self.send_json([p["id"] for p in State.platforms])
        if path == "/api/roms/identifiers":
            return self.send_json([r["id"] for r in State.roms])
        if path == "/api/roms":
            items = [r for r in State.roms if not after_t or r["updated_at"] > after_t[:19]]
            pids = {int(x) for x in q.get("platform_ids", [])}
            if pids:
                items = [r for r in items if r["platform_id"] in pids]
            limit = int(q.get("limit", ["50"])[0])
            offset = int(q.get("offset", ["0"])[0])
            return self.send_json({"items": items[offset:offset + limit], "total": len(items), "limit": limit, "offset": offset})
        if path.startswith("/api/roms/"):
            rid = path.rsplit("/", 1)[-1]
            for r in State.roms:
                if str(r["id"]) == rid:
                    return self.send_json(r)
        self.send_json({"detail": "Not Found"}, 404)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=8099)
    ap.add_argument("--games", type=int, default=1500)
    args = ap.parse_args()
    State.platforms, State.roms = build(args.games)
    print(f"mock RomM on http://localhost:{args.port} with {args.games} games", flush=True)
    ThreadingHTTPServer(("127.0.0.1", args.port), Handler).serve_forever()


if __name__ == "__main__":
    main()
