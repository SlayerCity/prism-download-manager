# Prism-local librqbit patch

Prism pins librqbit 9.0.1 and carries one narrow fork-local change in `src/session.rs`:
session-wide tracker fallbacks are not appended to torrents marked `private=1` in their
metainfo. This prevents Prism's configured public tracker list from announcing a private
info-hash to an unrelated tracker. Trackers in the torrent's own metainfo are retained.

This is not a claim of complete private-tracker support: peer protocol compatibility,
tracker announce behavior, ratio/seeding goals and live private-tracker validation remain
separate work. Revisit this patch when updating librqbit and retain its regression test.
