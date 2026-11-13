## Git workflow

- Commit and push directly to `origin/main` by default for a single, self-contained change, such as a small bug fix or an edit to `AGENTS.md`. Check the pull request criteria below before choosing this path.
- Commit completed, coherent changes autonomously using conventional commit messages.
- Push the working branch autonomously after each coherent milestone and before ending each development session. The pre-push hook runs during `git push`.
- Do not ask for approval before routine commits, pushes, refactoring, dependency changes, or other development activities.
- Never force-push, rewrite published Git history, delete remote branches, or perform destructive repository operations.
- Never bypass Git hooks, disable CI, remove failing tests, or weaken verification to make changes pass.
- When a commit changes something users can see or do in the app, CLI or site, add a line for it under **Unreleased** in [CHANGELOG.md](CHANGELOG.md) in the same commit. Write it for users, following the writing conventions below. Leave out internal changes such as CI, tooling, refactors, tests and contributor docs. These notes become the release notes, the update banner's What's New and the site's version list.
- [PROGRESS.md](PROGRESS.md) is the maintainer handoff, not a log. Update it only when current work, unresolved issues or next steps change, or at a release or milestone. Don't record routine fixes there: Git history and CHANGELOG.md cover them. Keep longer plans in `engineering/` and link to them. Use the [engineering index](engineering/README.md) to find maintained references and design decisions.

### When to use a pull request

- Use a PR when the change needs verification on a GitHub Actions runner beyond what the local hooks cover. Run the relevant workflows against the PR branch, trigger any manual workflows it needs, and confirm the checks passed for the latest revision before merging to `main`.
- Prefer a PR when you expect several rounds of iteration, such as debugging a tricky issue or refining a UI. Keep the intermediate commits on the branch and squash-merge the finished work into `main` as one conventional commit.
- Use a PR when the change needs a lasting explanation beyond a commit message. Record the problem, why you chose the approach, and how you verified it. Document any architectural boundaries or invariants the change affects.
- Use a PR when the diff is large or the work is complex, even if it has a single goal.

## Verification

- Run `prek install` in a new checkout or when hooks are missing. See [development setup](README.md#development) for dependencies. Run `just sync` after dependency changes so the pnpm lockfile hooks can check them offline.
- Let normal `git commit` and `git push` run the checks selected by `.pre-commit-config.yaml`. Pre-commit runs formatters and quick checks, commit-msg validates the message, and pre-push runs builds, linters, type checks, and tests.
- Do not run `just ci`, its alias `just check`, or the hook checks manually as a routine step before committing or pushing. Use `just ci` for diagnosing CI discrepancies or when explicitly requested.
- If a hook fails, fix the cause and retry the Git command. If a hook edits files, review and stage the intended edits before retrying the commit. If a push failure requires a code change, commit the fix and retry the push.
- Run focused checks during development or debugging. For example, use `pnpm --dir app test src/reader/progress.test.ts` for reader progress or `cargo test --locked -p parallax-device-kobo` for Kobo changes. Use the relevant manual or hardware checks when the change calls for them.

## Autonomous execution

- Continue working through the approved project roadmap without requesting permission at each milestone.
- Make routine implementation and architectural decisions independently, following the project specification.
- Prefer completed, tested features over partially implemented features.
- Use existing libraries and established solutions where appropriate rather than unnecessarily reimplementing functionality.
- Escalate only genuinely blocking ambiguities or operations that risk irreversible damage to external systems or real device data.

## Shared code and tests

- Keep library and device behavior shared by the desktop app and CLI in the Rust crates under `crates/`. Keep Tauri commands and CLI presentation in their respective entry points. When changing a CLI command that supports `--json`, keep stdout limited to the JSON payload and suppress progress output in JSON mode.
- Use fixtures and temporary directories for automated tests that write files, including `tempfile::TempDir` in Rust and `tmp_path` in Python. Isolate test libraries and device databases from real user data. Use the hardware checklist for physical-device verification and claim it only for workflows actually tested on hardware.

## Writing conventions

- Follow the Writing section of house-style's guidelines (`app/node_modules/house-style/GUIDELINES.md`, the release pinned in `app/package.json`) for UI copy, docs, release notes and the site. In-app copy is written for users, so say "you", lead with the answer, and use no em dashes.
- Write the README and landing page for first-time readers, `docs/` for users completing tasks, and `engineering/` for contributors. Link compatibility and performance claims to the relevant test results or measurements.
- Never hard-wrap Markdown. Write each paragraph and list item as a single line and let the editor soft-wrap. rumdl enforces this (`.rumdl.toml`) and its pre-commit hook joins wrapped lines automatically.

## Device work

- Route Kobo changes through the guarded device APIs. Use [fixture tests and the hardware checklist](engineering/hardware-checks.md) to verify changes. Cite the upstream source in comments when adapting calibre or Kobo Utilities logic.
- Use literal path comparisons in SQLite when matching device paths. `LIKE` treats underscores in file names as wildcards. See the [Kobo engineering notes](engineering/device-notes.md) for filesystem and database constraints.

## UI design

- Follow [the Parallax design system](engineering/design-system.md) and use `app/src/design/tokens.css` for shared type, color, spacing, control dimensions, radii, and motion. Do not introduce literal UI font sizes or inline typography overrides.
- Keep the main window and reader controls cohesive; publisher content and cover artwork retain the documented exceptions.
- Verify presentation at default and minimum window sizes; adapt layout instead of shrinking text. Keep the design contract tests in the standard verification path.
- Keep views mounted once shown and draw cached data on first render, following [retained views](engineering/decisions/retained-views.md): hide inactive tabs and sub-views instead of unmounting them, prefetch cheap library reads at launch, and read devices only when their view is on screen.

## Site

- `site/` is the Astro site at parallax.jacksonferguson.me. Unlike the app, it adopts house-style in full, as Star Ground's and jacksonferguson.me do: follow all of `site/node_modules/house-style/GUIDELINES.md` (the release pinned in `site/package.json`), use its tokens, shell, components and icons, and set only Parallax's gold accent in `site/src/styles/brand.css`.
- The hero shows the app's real library window, frozen from the browser mock by `just site-snapshot` into `site/public/app-snapshot/`. Rerun it after visible changes to the library window. Its books are the public-domain showcase in `site/scripts/showcase/`; only add covers with the same terms.
- The download button and the versions page read GitHub releases at build time (`site/src/lib/releases.ts`). Releases redeploy the site; never hard-code a version or download URL.
