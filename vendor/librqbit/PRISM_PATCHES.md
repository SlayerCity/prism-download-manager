# Prism-local librqbit patch

Prism pins librqbit 9.0.1 and carries two narrow fork-local changes:

- Session-wide public tracker fallbacks are not appended to torrents marked `private=1` in
their metainfo, preventing Prism from announcing a private info-hash to an unrelated tracker.
Trackers in the torrent's own metainfo are retained.
- Each managed torrent owns a shared live-adjustable upload/download limiter. It survives
pause/resume and can be changed while active; session-wide limits remain independently applied.

These changes are not a claim of complete private-tracker support: peer protocol compatibility,
ratio/seeding goals, port mapping and live private-tracker validation remain separate work.
Revisit the fork-local patch when updating librqbit and retain its regression tests.
