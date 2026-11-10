# Archive reader benchmark on Apple Silicon

Measured on November 9, 2026 on an Apple M3 with 8 CPU cores and 16 GiB RAM, running macOS 27.0.1. The benchmark executable used Rust 1.99.1 and Parallax's release settings of optimization level 3, LTO and one codegen unit.

## What Parallax links

`compress-tools` is a Rust interface to libarchive. Parallax uses it to read RAR and 7z book bundles and to convert CBR and CB7 comics to stored CBZ. The Rust `zip` reader handles ZIP bundles, CBZ and EPUB separately. `otool -L` on the local Parallax binary reports `/usr/lib/libarchive.2.dylib`. The pkg-config shim links against the macOS SDK, and macOS supplies the runtime library. A port to another OS would need to provide libarchive for that target or change the archive readers. [Linker shim](../../scripts/pkg-config-libarchive.sh) · [compress-tools](https://github.com/OSSystems/compress-tools-rs)

## Method

- **Book bundles** contained 14 real books, including three EPUBs, five AZW3s and six MOBIs, totalling 46.5 MiB. The workflow wrote one selected book at a time to temporary storage.
- **Comic volumes** contained 63 JPEGs and one PNG from a local manga volume, totalling 123.6 MiB. The workflow buffered one page at a time, wrote a stored CBZ through the same version of the Rust `zip` crate as Parallax, and flushed and synced the output.
- Both datasets included a small unrelated text file. RARLAB 7.23 generated RAR5 fixtures at normal compression (`-m3`) and store (`-m0`). RARLAB 6.24 generated legacy RAR4 fixtures (`-ma4 -m3`). 7-Zip 26.06 generated the 7z fixtures with LZMA2 at normal compression (`-m0=LZMA2 -mx=5`). Solid and non-solid runs used the same input data.
- The standalone harness used `compress-tools` 0.16.1 with macOS libarchive 3.7.4, `unrar-rs` 0.16.0 with `crypto-rust` and its default automatic decode policy, and `sevenz-rust2` 0.25.0 with one thread or eight threads. The native RAR bundle reader used the same forward-reading workaround as Parallax so a stored EPUB could not be mistaken for the outer archive.
- One warm-up and seven measured runs per case and backend, with backend order shuffled each round. Timings include archive open and traversal. Decode-only runs used a counting sink. Workflow runs also included writing output. The harness checked filenames, sizes and SHA-256 digests against the inputs, including the staged files and pages read back from each backend's CBZ. Peak RSS was collected with `/usr/bin/time -l`.
- These runs cover the archive stages of import. Library hashing, EPUB and Kindle parsing, cover generation, UI and Kobo transfer are outside the timings. The scratch SSD had less than 1 GiB free during measurement, so write timings are specific to this machine. The fixtures are local and the reads are warm-cache. Intel, Linux and Windows were outside this run.

Cells show median milliseconds (min–max) / median process peak RSS in MiB.

## RAR results

| Fixture | Stage | libarchive | unrar-rs |
| --- | --- | ---: | ---: |
| RAR5 compressed books (41.9 MiB packed) | Decode | 905.6 (902.1–907.4) / 70.4 | 66.0 (65.2–71.2) / 34.0 |
| RAR5 compressed books (41.9 MiB packed) | Stage books | 1,156.8 (1,096.5–1,615.2) / 70.4 | 301.4 (269.2–321.1) / 34.1 |
| RAR5 stored books (46.5 MiB packed) | Decode | 7.7 (7.6–8.0) / 6.4 | 3.7 (3.7–5.5) / 9.4 |
| RAR5 stored books (46.5 MiB packed) | Stage books | 174.9 (152.4–301.7) / 6.4 | 172.6 (143.9–236.2) / 14.4 |
| RAR4 compressed books (41.9 MiB packed) | Decode | 635.4 (633.2–703.9) / 12.8 | 193.8 (192.7–195.6) / 15.3 |
| RAR4 compressed books (41.9 MiB packed) | Stage books | 770.5 (755.5–799.3) / 12.8 | 345.2 (331.5–421.8) / 16.4 |
| RAR5 non-solid comic (122.2 MiB packed) | Decode | 2,346.0 (2,335.4–2,487.1) / 22.5 | 136.6 (133.3–139.2) / 18.7 |
| RAR5 non-solid comic (122.2 MiB packed) | Write CBZ | 2,737.7 (2,699.4–2,906.1) / 39.7 | 499.6 (471.4–557.8) / 43.2 |
| RAR5 solid comic (122.2 MiB packed) | Decode | 2,484.9 (2,445.9–2,504.8) / 38.4 | 151.2 (147.6–178.3) / 45.2 |
| RAR5 solid comic (122.2 MiB packed) | Write CBZ | 2,794.8 (2,774.1–2,950.4) / 58.3 | 478.2 (468.9–516.3) / 70.4 |
| RAR4 non-solid comic (122.2 MiB packed) | Decode | 1,646.2 (1,612.7–1,684.8) / 12.8 | 448.3 (441.9–523.6) / 12.3 |
| RAR4 non-solid comic (122.2 MiB packed) | Write CBZ | 1,954.7 (1,926.2–2,139.0) / 30.7 | 792.8 (755.3–820.1) / 39.3 |

## 7z results

| Fixture | Stage | libarchive | sevenz-rust2, 1 thread | sevenz-rust2, 8 threads |
| --- | --- | ---: | ---: | ---: |
| LZMA2 non-solid books (41.4 MiB packed) | Decode | 1,174.9 (1,167.5–1,210.6) / 24.8 | 1,086.2 (1,078.2–1,094.0) / 56.2 | 1,110.3 (1,071.2–1,136.6) / 148.8 |
| LZMA2 non-solid books (41.4 MiB packed) | Stage books | 1,280.7 (1,266.0–1,369.8) / 24.8 | 1,200.6 (1,183.5–1,232.7) / 56.2 | 1,224.9 (1,206.5–1,271.8) / 148.9 |
| LZMA2 solid comic (122.0 MiB packed) | Decode | 474.2 (473.3–479.7) / 38.4 | 412.1 (410.5–420.0) / 40.1 | 441.6 (439.3–444.9) / 349.2 |
| LZMA2 solid comic (122.0 MiB packed) | Write CBZ | 834.2 (768.0–920.3) / 60.0 | 740.5 (719.3–800.3) / 62.9 | 803.1 (760.0–831.3) / 366.5 |
| LZMA2 non-solid comic (122.0 MiB packed) | Decode | 369.5 (367.5–373.5) / 12.5 | 327.0 (325.3–331.3) / 36.2 | 348.0 (343.1–349.5) / 102.6 |
| LZMA2 non-solid comic (122.0 MiB packed) | Write CBZ | 697.8 (672.5–981.2) / 32.1 | 672.9 (629.4–713.2) / 47.0 | 657.4 (633.2–698.8) / 108.8 |

## Portability and release implications

- **RAR.** `unrar-rs` was 2.2–3.8× as fast at staging compressed books and 2.5–5.8× as fast at converting compressed comics to CBZ on these fixtures. The stored book bundle was effectively tied once both readers wrote the files. The pinned `unrar-rs` crate carries the original UnRAR restriction on recreating RAR compression. The pure-Rust crypto build avoids AWS-LC, but the license restriction still applies. Parallax is GPL-3.0-only, so shipping this linked reader would require resolving the additional restriction. [unrar-rs license](https://docs.rs/crate/unrar-rs/0.16.0/source/LICENSE) · [GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.html.en#GPLIncompatibleLibs)
- **7z.** One `sevenz-rust2` thread reduced median workflow times by 4–11%. Eight threads shaved 15 ms off the non-solid comic relative to one thread, while raising median peak RSS from 47 to 109 MiB. For the solid comic, eight threads took longer and raised RSS from 63 to 367 MiB. A portability-driven 7z switch would start with one thread and an explicit decoder memory limit. Its Apache-2.0 license has no UnRAR restriction. Parallax would still need libarchive for RAR and CBR until those paths use a different reader. [sevenz-rust2](https://github.com/hasenbanck/sevenz-rust2)
