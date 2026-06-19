# FractalChain CI Policy

**Status:** Active with T0 release-gate workflows

## Job Classes

PR-gated jobs run on `pull_request` and `push` to `main` / `master`.

- `ci.yml`: format, clippy, workspace tests, docs/script harness checks.
- `pilot-smoke.yml`: bounded two-shard pilot smoke and bounded
  masterchain-backed pilot smoke with log artifacts.
- `mvp-bridge-smoke.yml`: bounded 100-receipt MVP bridge smoke against the
  Docker devnet.

Nightly jobs run on schedule and by manual dispatch.

- `nightly-validation.yml`: masterchain, proof pipeline, light client, pilot
  smoke, and masterchain-backed pilot smoke.

## Time Bounds

- PR CI harness: 45 minutes.
- Pilot smoke: 35 minutes.
- Masterchain-backed pilot smoke: 40 minutes.
- MVP bridge smoke: 45 minutes.
- Nightly validation: 90 minutes.

Individual long-running script calls are wrapped with `timeout` so a hung node,
RPC server, or smoke test cannot consume the whole Actions runner indefinitely.

## Artifact Retention

- PR smoke logs: 14 days.
- MVP bridge compose logs: 14 days.
- Nightly validation logs: 30 days.

Artifacts are intended for release-gate debugging, not permanent archival.
Release sign-off artifacts should be copied into the release record for the tag
being approved.

## T0 Exit Evidence

T0 can be signed off when:

- Required workflows are present under `.github/workflows/`.
- PR-gated jobs pass on the release branch.
- The nightly workflow is scheduled and has at least one passing run, or an
  explicit release manager waiver records why the first nightly run will happen
  after branch cut.
- Failed smoke jobs upload logs.
