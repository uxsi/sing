# sing-box version pin

Pinned release: see `VERSION`.

Official core binaries are **not** vendored in git (large). Fetch them into the
Tauri sidecar path with:

```bash
./scripts/fetch-sing-box.sh
```

That writes `apps/desktop/src-tauri/binaries/sing-box-<target-triple>` for the
current host (and optionally both macOS triples). The desktop shell prefers the
bundled sidecar over Homebrew / PATH.
