# Parallax engineering

[User documentation](../docs/device-support.md) lives in `docs/`; the public site is built from `site/`. This directory holds contributor references and the evidence behind decisions. See [PROGRESS.md](../PROGRESS.md) for the current maintainer handoff.

## Maintained references

- [Design system](design-system.md) covers shared UI tokens, identity, layout, and visual verification.
- [App caching](app-caching.md) covers retained views, cache invalidation, and asynchronous refresh.
- [Kobo engineering notes](device-notes.md) cover the database, device writes, backups, and how to add a device report.
- [Releasing](releasing.md) covers cutting a release, signing keys and secrets, the site's deployment, and how the CLI is distributed.
- [Hardware checks](hardware-checks.md) provide repeatable procedures. [Hardware results](hardware-results.md) record which device, firmware, and workflows were actually tested.

## Decisions and measurements

- [EPUB reader architecture](decisions/epub-reader.md)
- [Comic preparation for Kobo](decisions/kobo-comics.md)
- [Archive reader benchmark on Apple Silicon](benchmarks/archive-reader-benchmark.md)

## Historical material

These links point to the last committed snapshot before the documentation consolidation. The plans and experiments describe the project at the time they were written.

| Material | Snapshot |
| --- | --- |
| Original scope and implementation plans | [Project brief](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/parallax-project.md), [Kobo MVP plan](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/docs/PLAN.md), [Comics plan](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/docs/comics-plan.md) |
| EPUB reader investigation | [Report](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/reports/EPUB%20reader%20library%20landscape.md), [Research notes](https://github.com/JacksonFergusonDev/parallax/tree/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/research_notes/EPUB%20reader%20library%20landscape), [Spike results](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/docs/reader-spike.md), [Prototype](https://github.com/JacksonFergusonDev/parallax/tree/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/spikes/reader) |
| Kobo comics investigation | [Report](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/reports/Bindery%20and%20Kindle%20Comic%20Converter.md), [Research notes](https://github.com/JacksonFergusonDev/parallax/tree/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/research_notes/Bindery%20and%20Kindle%20Comic%20Converter), [Spike results](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/docs/kobo-comics-spike.md), [Prototype](https://github.com/JacksonFergusonDev/parallax/tree/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/spikes/kobo-comics) |
| Early Kobo hardware work | [Reference device inventory](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/docs/device-notes.md), [Original hardware checklist](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/docs/hardware-checks.md), [Collection experiment script](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/scripts/spike-s1.sh) |
| Implementation diary through November 10, 2026 | [Original progress log](https://github.com/JacksonFergusonDev/parallax/blob/8b6e216f41368f0936ea1b2a7acccfb7f24d272a/PROGRESS.md) |
