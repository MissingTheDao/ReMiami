Characterisation data recorded from original C++ code before it was replaced by Rust. The Rust
tests replay every line, so the port keeps the exact behaviour (quirks included).

| File | Recorded from | Checked by |
|---|---|---|
| `math.txt` | src/math (CVector, CQuaternion, CMatrix, TransformPoint, compressed matrices): 40 functions x 150 cases (random rotations, scaled and arbitrary matrices, special values), floats as hex bits; products' uninitialised w column masked | `rust/revc/src/math.rs`, test `golden::matches_cpp_golden` (bit for bit) |
| `messages.txt` | CMessages' queues (brief and big messages, previous briefs): 4000 random operations, state after each | `rust/revc/src/messages.rs`, test `golden::matches_cpp_golden` |
| `pager.txt` | CPager (scrolling pager messages): 4000 random operations with the HUD/widescreen "blocked" state, pager and previous-briefs state after each | `rust/revc/src/pager.rs`, test `golden::matches_cpp_golden` |
| `savecheck.txt` | CheckDataNotCorrupt on 405 mutations of `tests/saves/GTAVCsf1.b` (recipes: byte flips, truncation, block sizes incl. negative and oversized, fixed-up checksums, trailing bytes) | `rust/revc/src/savefile.rs`, test `golden::matches_cpp_golden` |
| `textutil.txt` | CMessages string helpers and Text.cpp's UnicodeToAscii* / GetUpperCase (commit before the text port) | `rust/revc/src/textutil.rs`, test `matches_cpp_golden` |

Line format: `op flags args => result`, strings as comma-separated hex units (`-` = empty).
The recorder (a temporary C++ file calling the originals plus an ignored Rust test) was removed
with the C++ code; see the git history of the commit that added this file.

The math recorder is kept: `tests/golden/recorders/math/record.sh` rebuilds it against the C++ from
git history and rewrites `math.txt` (the result must not change).
