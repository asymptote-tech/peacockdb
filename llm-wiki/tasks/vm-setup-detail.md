# vm-setup — run detail

Branch `ENS-vm-setup`, forked from `ENS-containers-only` at `7aeec989`; its PR targets
`ENS-containers-only`.

## Progress

- 2026-11-09: spec, plan and board entry committed (`fed95745`); Task 1 committed (`ad19c967`).
- 2026-11-09: Task 2 on dev, below. Dev's docker keeps the branch's 25.02 build and run images
  (inputs tags below) for Tasks 13, 14 and 16.

## Plain builds

On dev, 2026-11-09, at `ad19c967`, cuDF 25.02. Each `image.sh build` was followed at once by the
plain build of the same Dockerfile from the repository's root, with no build args.

| kind | `image.sh build <kind> 25.02` | plain `docker build -f docker/<kind>.Dockerfile .` | RUN/COPY steps, CACHED | sha256 of `RootFS.Layers` JSON, both images |
|---|---|---|---|---|
| build | 1659 s, `peacockdb-build:cudf-25.02-in-525f460c6dd5` | 2 s | 17 of 17 | `32ce27a87627f6daf76224fe89fd52d7b9cd53d6190a3ff03ece5c4c1e572d33` |
| run | 248 s, `peacockdb-run:cudf-25.02-in-febbd8e327d9` | 3 s | 8 of 8 | `a57a2838d613966a2de772f15f23285603cedb51ba4b4c71e61e14181416077c` |

- The run image's `peacockdb.driver-minimum-ptx` label reads `575.51.03`.
- `docker build -f docker/run.Dockerfile --build-arg CUDF=25.10 .` exits 1 at its final `FROM`:
  `ERROR: failed to build: failed to solve: run-25.10: failed to resolve source metadata for
  docker.io/library/run-25.10:latest: pull access denied, repository does not exist or may require
  authorization`.
- The plain images' tags were removed after the comparison.
