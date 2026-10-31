## Git workflow

- Work directly on `main`. This is a solo-development repository; feature branches and pull requests are not required.
- Commit completed, coherent changes autonomously using conventional commit messages.
- Push to `origin/main` autonomously after verified milestones and before ending each development session.
- Do not ask for approval before routine commits, pushes, refactoring, dependency changes, or other development activities.
- Run relevant formatting, linting, type-checking, and tests before committing. Resolve failures independently.
- Never force-push, rewrite published Git history, delete remote branches, or perform destructive repository operations.
- Never bypass Git hooks, disable CI, remove failing tests, or weaken verification to make changes pass.
- Maintain a concise progress file documenting completed milestones, current work, unresolved issues, and next steps.

## Autonomous execution

- Continue working through the approved project roadmap without requesting permission at each milestone.
- Make routine implementation and architectural decisions independently, following the project specification.
- Prefer completed, tested features over partially implemented features.
- Use existing libraries and established solutions where appropriate rather than unnecessarily reimplementing functionality.
- Escalate only genuinely blocking ambiguities or operations that risk irreversible damage to external systems or real device data.
- Verify functionality through automated tests wherever possible.
- Do not claim real-device functionality has been verified unless tested against actual hardware.
