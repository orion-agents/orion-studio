# Orion Studio Release Policy

This policy applies to every Orion Studio artifact advertised as a release,
including Stable, Preview, and emergency patch releases. It is a release gate
for people and agents: a Git tag, GitHub Release, or downloadable DMG alone is
not a releasable product.

## Non-negotiable trust requirement

Every public macOS release must be signed with the organization's **Developer
ID Application** certificate, accepted by Apple notarization, stapled, and
accepted by macOS Gatekeeper. A build missing any one of these properties must
remain unpublished and must not be described as a release.

The required release chain is:

```text
reviewed main commit
  -> cryptographically signed annotated tag
  -> protected CI environment
  -> Developer ID signature
  -> Apple notarization and stapling
  -> clean-machine verification
  -> published GitHub Release
```

## Version and channel rules

| Release type | Immutable tag | `crates/zed/RELEASE_CHANNEL` |
| --- | --- | --- |
| Stable | `vMAJOR.MINOR.PATCH` | `stable` |
| Preview | `vMAJOR.MINOR.PATCH-pre` | `preview` |

The tag must point to a reviewed commit reachable from `main`. It must be an
annotated, cryptographically signed tag, verified against a trusted maintainer
key before it is pushed. Moving, deleting, or reusing a published tag is
prohibited; publish a new patch version instead.

## Mandatory release gates

An agent may proceed only when each applicable gate is recorded as passing:

1. **Source gate** — the exact commit is reviewed, merged to `main`, clean of
   unrelated changes, and has passed the release test/build checks.
2. **Identity gate** — the tag is annotated and signature verification reports
   a trusted maintainer identity.
3. **Protected-environment gate** — GitHub Actions runs in the protected
   `production` environment for Stable, or the protected `preview` environment
   for Preview. Signing and notarization credentials stay only in environment
   secrets; they must never be committed, printed, or uploaded as artifacts.
4. **Signing gate** — the `.app` is signed with the organization's Developer
   ID Application identity using hardened runtime. The DMG is signed when the
   release workflow produces one.
5. **Notarization gate** — Apple returns an `Accepted` notarization result and
   the final distributable has a stapled ticket.
6. **Artifact gate** — the release includes the intended app/DMG and a
   SHA-256 checksum generated from that exact uploaded artifact.
7. **Gatekeeper gate** — on a clean macOS user profile or clean VM, the
   downloaded artifact passes both signature and Gatekeeper assessment:

   ```bash
   codesign --verify --deep --strict --verbose=2 "/Applications/Orion Studio.app"
   spctl --assess --type execute --verbose=4 "/Applications/Orion Studio.app"
   stapler validate "/Applications/Orion Studio.app"
   ```

   If the release distributes a DMG, run `stapler validate` against the DMG as
   well before publishing it.
8. **Publication gate** — a maintainer confirms the workflow result, checksums,
   notarization evidence, and Gatekeeper evidence, then publishes the GitHub
   Release. Stable releases remain drafts until this final review is complete.

## Credentials and approvals

- The Developer ID certificate's private key and any exported `.p12` are
  secrets. Keep them in approved keychains or GitHub environment secrets only.
- App Store Connect `.p8` keys, key IDs, issuer IDs, passwords, and secret
  values must never appear in source, documentation, terminal output, PRs, or
  release notes.
- Agents may prepare commits, tags, and draft-release metadata, but may not
  bypass a protected-environment approval or substitute local credentials for
  the protected workflow.
- An emergency release follows the same signing, notarization, checksum, and
  Gatekeeper gates. Urgency does not waive product trust requirements.

## Explicitly prohibited shortcuts

Do not publish an unsigned build, an ad-hoc-signed build, an unstapled build,
or a build whose notarization result is pending or rejected. Do not instruct
users to remove quarantine attributes, bypass Gatekeeper, or use an unverified
mirror. Do not treat a successful local build, GitHub artifact upload, or
created release draft as release completion.

## Required release evidence

The GitHub Release or its linked release record must retain:

- immutable tag and commit SHA;
- CI run URL and successful job results;
- signing identity subject/team ID (never private material);
- notarization submission/result identifier and `Accepted` status;
- stapling and Gatekeeper verification output or an equivalent retained log;
- artifact filenames, sizes, and SHA-256 checksums; and
- concise release notes that accurately distinguish delivered functionality
  from planned follow-up work.

If any evidence is absent or any gate fails, stop the release, preserve the
logs, fix the issue on `main`, and issue a new signed patch tag after review.
