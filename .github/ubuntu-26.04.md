# Ubuntu 26.04 runner migration

GitHub's [runner-images announcement](https://github.com/actions/runner-images/issues/14748)
sets the `ubuntu-latest` migration window to October 19–November 19, 2026.
Tests, E2E, Clippy and Fmt are pinned to `ubuntu-24.04` while compatibility is
being validated. The required `ubuntu_test` check keeps its existing name.

## Probe

`Ubuntu 26.04 compatibility` calls the actual Tests and E2E workflows with
`runner: ubuntu-26.04`. It runs on pull requests changing the probe or either
workflow, and can be rerun from Actions → Ubuntu 26.04 compatibility → Run
workflow. After this workflow reaches `nightly`, it can also be dispatched for
a migration branch:

```sh
gh workflow run ubuntu-26.04.yml --repo next-hat/nanocl --ref <branch>
```

Both jobs must pass; failures are not hidden with `continue-on-error`. Keep this
optional probe out of required branch checks until migration is complete.
The probe inherits no repository secrets and skips Codecov upload. Buildx and
Rust caches are separated by runner image. It uses the same Docker images,
Compose setup, `/run/nanocl` sockets, coverage tests and Bats suite as normal CI.
The downloaded Ubuntu **24.04 guest** image intentionally remains unchanged:
the experiment changes the runner host, not the VM fixture.

## Deliberate migration follow-up

- Record both probe run URLs, commit SHA and runner image versions in the PR.
- Compare failures with the 24.04 Tests and E2E jobs on the same commit. Fix only
  demonstrated host-specific failures; retain the pin if either path fails or
  cannot be run. An unrelated failure is not proof of incompatibility or success.
- Before lifting the pin, run Clippy and Fmt on 26.04 as well, then deliberately
  change all four normal jobs to `ubuntu-26.04` in a reviewed migration PR.
- Rerun normal Tests and E2E after that change. Remove the temporary probe once
  the new host is established. Keep the 24.04 guest fixture unless its own
  migration is being tested separately.
