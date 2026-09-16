# Fork release versioning

This policy applies to `wkentaro/herdr` releases from `0.9.0-fork.1` onward.

## Canonical version

Use `<upstream-version>-fork.<revision>` as the Cargo and displayed version, with
a leading `v` for the Git tag. Start the revision at 1 for each new upstream
version and increment it for subsequent fork releases based on that version.
Continue from any existing fork revisions rather than reusing a published tag.

| Upstream | Fork revision | Git tag |
| --- | --- | --- |
| `0.9.0` | 1 | `v0.9.0-fork.1` |
| `0.9.0` | 2 | `v0.9.0-fork.2` |
| `0.9.1` | 1 | `v0.9.1-fork.1` |

Keep the upstream version and fork revision explicit. Packing them into one patch
number obscures the upstream base, makes patch zero ambiguous with upstream tags,
and imposes an arbitrary limit on one of the counters.

[Cargo requires SemVer syntax](https://doc.rust-lang.org/cargo/reference/manifest.html#the-version-field),
so a four-component version such as `0.9.0.1` cannot be the canonical Cargo version.

## Windows representation

Convert only at a boundary that requires numeric Windows file-version metadata:
`<major>.<minor>.<patch>-fork.<revision>` maps to
`<major>.<minor>.<patch>.<revision>`. For example, `0.9.0-fork.2` maps to `0.9.0.2`.
Retain the canonical version in tags, Cargo, and human-readable output on every
platform, including Windows.

Each numeric file-version component must fit in `0..=65535`.
[Windows file-version resources support four 16-bit components](https://learn.microsoft.com/en-us/windows/win32/menurc/versioninfo-resource)
and [a separate human-readable string](https://devblogs.microsoft.com/oldnewthing/20230503-51/?p=108135).
Identify the consumer requiring conversion before introducing it; a Windows ZIP
release alone does not establish that requirement.

The current Windows ZIP pipeline accepts the canonical suffix without conversion.
The [Windows build for `v0.8.2-fork.4`](https://github.com/wkentaro/herdr/actions/runs/33721572315/job/100541619826)
succeeded; that release's failed jobs were Nix validation and website publication.
Keep numeric conversion conditional on adding a consumer that actually needs it.

This mapping is for executable file-version metadata. An MSI installer would need
a separate policy because [MSI ignores the fourth component](https://learn.microsoft.com/en-us/windows/win32/msi/productversion).

## Update compatibility and migration

The updater must parse and compare fork suffixes on every platform, using numeric
revision ordering: `fork.2 < fork.10`. Update discovery must use the fork's release
source. Under [SemVer precedence](https://semver.org/#spec-item-11),
`0.9.0-fork.2 < 0.9.0`; the suffix does not make the fork newer than the matching
upstream release.

Direct installs use the stable channel and the fork's GitHub release asset
`latest.json`, which includes all five platform assets and their SHA-256 checksums.
Preview updates are unavailable for fork builds. Remote installation selects the
manifest attached to the client's exact release tag, preserving version identity
even after newer fork releases appear. Plugin minimum-version checks accept the
upstream base API version independently of fork release ordering.

Preserve published tags and ensure the first release under this policy sorts
after the existing fork release in the same update stream. For example,
`0.9.0-fork.1` follows `0.8.201`, while `0.8.2-fork.5` sorts below `0.8.201`.
Verify suffix parsing, revision ordering, and migration from the previously
published version before releasing with this policy.

Upgrading from `0.8.201` requires a one-time manual installation of the matching
fork release asset: that version checks upstream and cannot parse fork suffixes.
On Windows, extract the whole ZIP so the executable retains its app-local ConPTY
runtime. Subsequent direct installs use the fork update feed described above.

The upstream `just release` recipes require plain versions and `master`. Prepare
fork releases on `fork`, run `just check` and `just pre-release-check`, then commit
the matching Cargo version, lockfile, and changelog before tagging. Pushing the
tag runs the fork-enabled build and release jobs; upstream issue closure and
website publication remain disabled for the fork.

For the performance check after rebasing, set `HERDR_PERF_BASELINE_BIN` to the
published binary for the upstream base release. The checked-in distribution
manifest can refer to an older release and is not the comparison for that rebase.
