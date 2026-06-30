## Summary

<!-- What does this PR do and why? Link to any design doc or discussion. -->

## Related Issues

Closes #

## Type of Change

- [ ] `feat` — new user-facing feature (minor bump)
- [ ] `fix` — bug fix (patch bump)
- [ ] `refactor` — code restructuring, no behaviour change
- [ ] `perf` — performance improvement
- [ ] `test` — tests only
- [ ] `ci` — workflow / tooling change
- [ ] `docs` — documentation only
- [ ] `chore` — deps, maintenance

## What changed

<!-- A brief bullet list of the concrete changes. Focus on the WHY, not the what — the diff shows the what. -->

-
-

## How to test

<!-- Steps a reviewer can take to verify the change works. For Rust: which cargo commands? For provider changes: what Proxmox / Hetzner setup is needed? -->

1.
2.

## Checklist

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features` is clean (zero warnings)
- [ ] `cargo test --all` passes
- [ ] If frontend changed: `npm run type-check`, `npm run lint`, `npm run build` all pass
- [ ] Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`type(scope): description`)
- [ ] Breaking changes are documented with a `!` in the commit type and a `BREAKING CHANGE:` footer

## Screenshots / logs

<!-- For UI changes: before/after screenshots. For provider/operator changes: relevant log output. Delete this section if not applicable. -->
