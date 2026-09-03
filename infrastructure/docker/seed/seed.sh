#!/usr/bin/env bash
# Seed demo data through the gateway. Idempotent: safe to re-run.
#
#   just seed          # against http://localhost:8080
#   GATEWAY_URL=... just seed
#
# Creates: one studio (ACME Bros Pictures), one label (Warp Records), an
# admin, a movie with two scenes, and three songs. Leaves licensing open —
# run the negotiation yourself or via the e2e suite.
set -euo pipefail

GATEWAY="${GATEWAY_URL:-http://localhost:8080}"
CURL="curl -fsS"

json() { python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }

login() { # login EMAIL PASSWORD -> access token
    $CURL -X POST "$GATEWAY/auth/login" \
        -H 'content-type: application/json' \
        -d "{\"email\":\"$1\",\"password\":\"$2\"}" | json 'd["access_token"]'
}

authed() { # authed TOKEN METHOD PATH [JSON_BODY]
    local token="$1" method="$2" path="$3" body="${4:-}"
    if [ -n "$body" ]; then
        $CURL -X "$method" "$GATEWAY$path" \
            -H "Authorization: Bearer $token" \
            -H 'content-type: application/json' \
            -d "$body"
    else
        $CURL -X "$method" "$GATEWAY$path" \
            -H "Authorization: Bearer $token"
    fi
}

echo "==> ensuring accounts (registers on first run, 409s after)"
$CURL -X POST "$GATEWAY/auth/register" -H 'content-type: application/json' -d '{
    "email": "grace@acme.example", "password": "nw-derulo-99",
    "display_name": "Grace (Studio)", "role": "STUDIO", "org_name": "ACME Bros Pictures"
}' > /dev/null 2>&1 || true
$CURL -X POST "$GATEWAY/auth/register" -H 'content-type: application/json' -d '{
    "email": "warp-label@acme.example", "password": "label-pass-99",
    "display_name": "Warp Records A&R", "role": "LABEL", "org_name": "Warp Records"
}' > /dev/null 2>&1 || true
$CURL -X POST "$GATEWAY/auth/register" -H 'content-type: application/json' -d '{
    "email": "admin@acme.example", "password": "admin-pass-99",
    "display_name": "Platform Admin", "role": "ADMIN"
}' > /dev/null 2>&1 || true

STUDIO=$(login grace@acme.example nw-derulo-99)
LABEL=$(login warp-label@acme.example label-pass-99)
echo "    tokens ok"

echo "==> seeding songs (label: Warp Records)"
seed_song() { # seed_song TITLE AUTHOR SECONDS
    local existing
    existing=$(authed "$LABEL" GET "/songs" | json "[s['title'] for s in d].count('$1')")
    if [ "$existing" = "0" ]; then
        authed "$LABEL" POST /songs \
            "{\"title\":\"$1\",\"author\":\"$2\",\"length_seconds\":$3}" | json 'd["id"]'
    else
        authed "$LABEL" GET /songs | json "[s['id'] for s in d if s['title']=='$1'][0]"
    fi
}
SONG_1=$(seed_song "Nightcall" "Kavinsky" 252)
SONG_2=$(seed_song "Sunset Vertical" "Com Truise" 281)
SONG_3=$(seed_song "Turbo Killer" "Carpenter Brut" 280)
echo "    songs: $SONG_1 $SONG_2 $SONG_3"

echo "==> seeding movie with two scenes (studio: ACME Bros Pictures)"
MOVIE_TITLE="Neon Pursuit"
MOVIE_EXISTS=$(authed "$STUDIO" GET /movies | json "[m['title'] for m in d].count('$MOVIE_TITLE')")
if [ "$MOVIE_EXISTS" = "0" ]; then
    MOVIE=$(authed "$STUDIO" POST /movies \
        "{\"title\":\"$MOVIE_TITLE\",\"description\":\"A rain-soaked chase through a neon city.\"}" | json 'd["id"]')
else
    MOVIE=$(authed "$STUDIO" GET /movies | json "[m['id'] for m in d if m['title']=='$MOVIE_TITLE'][0]")
fi
SCENE_COUNT=$(authed "$STUDIO" GET "/movies/$MOVIE" | json 'len(d["scenes"])')
if [ "$SCENE_COUNT" = "0" ]; then
    authed "$STUDIO" PUT "/movies/$MOVIE/scenes" \
        '{"screen_time_seconds":180,"start_time_seconds":0,"end_time_seconds":180,"description":"The pursuit begins"}' > /dev/null
    authed "$STUDIO" PUT "/movies/$MOVIE/scenes" \
        '{"screen_time_seconds":90,"start_time_seconds":180,"end_time_seconds":270,"description":"Rooftop standoff"}' > /dev/null
fi
echo "    movie: $MOVIE (2 scenes)"

echo "seed complete."
echo "  studio: grace@acme.example / nw-derulo-99"
echo "  label:  warp-label@acme.example / label-pass-99"
echo "  admin:  admin@acme.example / admin-pass-99"
