"""Generate the labelled matcher set (tasks.md T038).

Synthetic but realistic: real songs, with search results arranged in the traps the matcher has to
handle (covers ranked first, live/remix/sped-up versions, long video intros, missing artist in the
title, Spotify "Remastered" suffixes). Each case lists every acceptable video id (the same
recording: Topic upload, official video, lyric video). Deterministic: re-running gives the same file.

    python3 generate.py > cases.json
"""
import json, random

SONGS = [
    ("Queen", "Bohemian Rhapsody", 354), ("Survivor", "Eye of the Tiger", 245), ("Guns N' Roses", "Sweet Child O' Mine", 356),
    ("Europe", "The Final Countdown", 310), ("Michael Jackson", "Billie Jean", 294), ("The Weeknd", "Blinding Lights", 200),
    ("Ed Sheeran", "Shape of You", 234), ("Adele", "Rolling in the Deep", 228), ("Daft Punk", "Get Lucky", 369),
    ("Coldplay", "Viva la Vida", 242), ("Nirvana", "Smells Like Teen Spirit", 301), ("a-ha", "Take On Me", 225),
    ("Dua Lipa", "Levitating", 203), ("Billie Eilish", "bad guy", 194), ("Beyoncé", "Halo", 261),
    ("Sigur Rós", "Hoppípolla", 268), ("Rosalía", "Malamente", 150), ("Bad Bunny", "Tití Me Preguntó", 243),
    ("Caetano Veloso", "Sozinho", 197), ("Anitta", "Envolver", 193), ("Tom Jobim", "Águas de Março", 212),
    ("Stromae", "Alors on danse", 206), ("Radiohead", "Creep", 238), ("Oasis", "Wonderwall", 258),
    ("Arctic Monkeys", "Do I Wanna Know?", 272), ("Taylor Swift", "Anti-Hero", 200), ("Harry Styles", "As It Was", 167),
    ("Kendrick Lamar", "HUMBLE.", 177), ("Eminem", "Lose Yourself", 326), ("Fleetwood Mac", "Dreams", 257),
    ("ABBA", "Dancing Queen", 231), ("Bee Gees", "Stayin' Alive", 285), ("Whitney Houston", "I Wanna Dance with Somebody (Who Loves Me)", 291),
    ("Bob Marley & The Wailers", "Is This Love", 232), ("Linkin Park", "Numb", 186), ("Gorillaz", "Feel Good Inc.", 222),
    ("Lady Gaga", "Bad Romance", 294), ("Rihanna", "Umbrella", 275), ("Avicii", "Wake Me Up", 247),
    ("Kygo", "Firestone", 271), ("Imagine Dragons", "Believer", 204), ("Lorde", "Royals", 190),
    ("Post Malone", "Circles", 215), ("SZA", "Kill Bill", 153), ("Olivia Rodrigo", "drivers license", 242),
    ("Måneskin", "Beggin'", 211), ("Shakira", "Hips Don't Lie", 218), ("Daddy Yankee", "Gasolina", 192),
    ("Tame Impala", "The Less I Know the Better", 216), ("Mac DeMarco", "Chamber of Reflection", 231),
]

rng = random.Random(20261011)
_ids = set()

def vid():
    while True:
        v = "".join(rng.choice("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_") for _ in range(11))
        if v not in _ids:
            _ids.add(v)
            return v

def entry(title, channel, dur):
    return {"_type": "url", "ie_key": "Youtube", "id": vid(), "title": title, "channel": channel, "duration": dur}

def topic(a, t, d): return entry(t, f"{a} - Topic", d + rng.choice([-1, 0, 0, 1]))
def official(a, t, d, intro=0): return entry(f"{a} - {t} (Official Video)", f"{a.replace(' ', '')}VEVO" if rng.random() < .5 else a, d + intro)
def lyric(a, t, d): return entry(f"{a} - {t} (Lyrics)", rng.choice(["7clouds", "Taj Tracks", "Lyrics Vault", "Dan Music"]), d + rng.choice([-2, 0, 1, 2]))
def live(a, t, d): return entry(f"{a} - {t} (Live at Wembley)", a, d + rng.randint(20, 90))
def cover(a, t, d): return entry(f"{t} - {a} | Cover by {rng.choice(['Jane Doe', 'Boyce Avenue', 'Alex G', 'Sam Tsui'])}", rng.choice(["Boyce Avenue", "Jane Doe Music", "Alex G"]), d + rng.randint(-15, 15))
def karaoke(a, t, d): return entry(f"{t} (Karaoke Version) - {a}", "Sing King", d + rng.randint(-3, 3))
def remix(a, t, d): return entry(f"{a} - {t} (Remix)", "Remix Nation", d + rng.randint(30, 90))
def sped(a, t, d): return entry(f"{t} (sped up)", "speedy tunes", int(d * 0.8))

def source(a, t, d, **kw):
    s = {"title": t, "artists": [a], "duration_ms": d * 1000 + rng.randint(-400, 400)}
    s.update(kw)
    return s

cases = []
for a, t, d in SONGS:
    # 1. Topic upload exists but a live version and a cover rank above it.
    tp, of, ly = topic(a, t, d), official(a, t, d, intro=rng.choice([0, 4, 12])), lyric(a, t, d)
    cands = [live(a, t, d), cover(a, t, d), tp, of, ly]
    cases.append({"scenario": "topic_below_traps", "source": source(a, t, d), "candidates": cands,
                  "acceptable": [tp["id"], of["id"], ly["id"]]})
    # 2. Spotify-style remaster suffix; only video uploads (no Topic), karaoke ranked first.
    of2, ly2 = official(a, t, d, intro=rng.choice([0, 2, 8])), lyric(a, t, d)
    cases.append({"scenario": "remaster_suffix", "source": source(a, f"{t} - Remastered 2011", d),
                  "candidates": [karaoke(a, t, d), cover(a, t, d), of2, ly2],
                  "acceptable": [of2["id"], ly2["id"]]})
    # 3. Title has no artist; the channel names the artist. Sped-up and remix uploads around it.
    plain = entry(t, a, d + rng.choice([0, 1, 2]))
    cases.append({"scenario": "artist_in_channel", "source": source(a, t, d),
                  "candidates": [sped(a, t, d), remix(a, t, d), plain],
                  "acceptable": [plain["id"]]})
    # 4. The user wants the remix: the original must lose.
    rmx_len = d + 40
    want = entry(f"{a} - {t} (Remix)", "Remix Nation", rmx_len + 1)
    cases.append({"scenario": "wants_remix", "source": {"title": f"{t} (Remix)", "artists": [a], "duration_ms": rmx_len * 1000},
                  "candidates": [official(a, t, d), topic(a, t, d), want],
                  "acceptable": [want["id"]]})
for a, t, d in SONGS[:10]:
    # 5. Featured artist in the source, ISRC known; official video with long intro (+25 s).
    of = official(a, t, d, intro=25)
    cases.append({"scenario": "long_intro_feat", "source": source(a, f"{t} (feat. Someone)", d, isrc="USUM71900001"),
                  "candidates": [cover(a, t, d), of, live(a, t, d)],
                  "acceptable": [of["id"]]})

print(json.dumps(cases, indent=1, ensure_ascii=False))
