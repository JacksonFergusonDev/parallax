# Comic archive fixtures

- `sample.cb7`: a 7z comic made with macOS's `bsdtar --format 7zip`. Three tiny greyscale PNG pages (`Vol 1/p1.png`, `p2.png`, `p10.png`, in a wrapper folder), a `notes.txt` and a ComicInfo.xml with series "Fixture", volume 1 and `Manga=YesAndRightToLeft`.
- `libarchive-rar4.rar` and `libarchive-rar5.rar`: `test_read_format_rar.rar` and `test_read_format_rar5_compressed.rar` from libarchive 3.7.4's test suite (BSD 2-clause, © the libarchive authors). They hold text and binary files rather than pages, and show that macOS's libarchive reads RAR4 and compressed RAR5. RAR files can't be made without the proprietary `rar` tool.
