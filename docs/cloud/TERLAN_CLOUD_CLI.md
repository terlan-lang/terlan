# Terlan Cloud CLI

Terlan Cloud accepts Terlan applications only. The public compiler owns the
build and client workflow; Cloud owns authentication, release admission,
deployment policy, execution, and audit history.

## Login

Pass credentials through private files, never command-line token values:

```bash
terlc login \
  --cloud https://cloud.terlan.dev \
  --user-id usr_0123456789abcdef \
  --token-file /private/path/cloud-api-token \
  --artifact-token-file /private/path/cloud-artifact-token
```

On Unix, token files must be regular files inaccessible to group and other
users. The resulting profile is stored with mode 0600 under
`$XDG_CONFIG_HOME/terlan/cloud.json`, `~/.config/terlan/cloud.json`, or the path
selected by `TERLAN_CLOUD_PROFILE`/`--profile`.

Public Cloud origins require HTTPS. Loopback HTTP is accepted only for local
development. Redirects are disabled so credentials are never forwarded to a
different origin.

## Deploy

From a project containing `terlan.toml`:

```bash
terlc deploy . --out-dir _build
```

The default path performs a release build, creates a deterministic zstd archive
from the compiler checksum inventory, uploads it through authenticated artifact
admission, verifies the validated response, and creates the deployment. The
manifest must identify a public HTTPS source repository, or it must be supplied
with `--repository`; a Git revision is detected from the project or supplied
with `--revision`.

An operator may deploy an already admitted release without rebuilding it:

```bash
terlc deploy . \
  --project terlan-registry \
  --release-id rel_0123456789abcdef
```

Successful deployment stores the last project and deployment identity in the
profile.

## Observe and roll back

The last successful deployment is the default for:

```bash
terlc status
terlc logs --lines 100
terlc rollback
```

Use `--project`, `--deployment`, or `--profile` to select an explicit target.
Log reads are bounded to 1–1,000 lines. Rollback records durable intent in
Cloud; the workflow worker performs the state transition and route restoration.

The provider-free acceptance gate is maintained in the Terlan Cloud workspace:

```bash
make hosted-cli-loop-check
```

It builds and uploads the Registry from Terlan source, promotes it through the
shared Pingora gateway, checks status and release-scoped logs, rolls it back,
and proves Registry archive/index/snapshot continuity.
