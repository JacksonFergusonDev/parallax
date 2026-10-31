# E-Reader Companion — Project Brief

## Goal

Build a fast, beautiful, intuitive desktop app for managing ebooks on **Kobo and Kindle** devices. The main workflow is simple: import an EPUB, improve its cover, convert it if needed, transfer it to an e-reader, and organize the library **on the device**.

This should be a focused alternative to calibre's complexity, not a full calibre replacement.

## Features

### Book preparation

- Import local EPUB files via drag-and-drop, including multiple books at once.
- View and edit basic metadata: title, author, series, genre/tags.
- Replace poor-quality covers with local images or find higher-resolution alternatives online.
- Convert automatically to an appropriate device format, especially **EPUB → KEPUB for Kobo**, with suitable Kindle conversion/delivery options.

### Device and library management

- Detect connected Kobo and Kindle devices and browse their installed books.
- Transfer, remove, and manage books with clear feedback.
- Make native e-reader organization genuinely easy: collections, genres, series, bulk edits, and optional automatic grouping rules.
- Where possible, synchronize groupings to the collections users see on the device, rather than merely organizing files in desktop folders.
- Respect differences between devices. In particular, don't pretend Kindle supports native collection management when it doesn't; clearly distinguish local-only organization from on-device changes.

### Reading insights

- Show reading status (unread, in progress, finished), percentage completed, and last-read date where accessible.
- Track how long books have been on a device.
- Explore reading history and statistics, including time spent reading, when device data permits. Detailed analytics are optional and must not rely on invented or unreliable data.

## User experience

Prioritize a **snappy, polished, minimal interface**: quick startup, smooth navigation, fast search, attractive book-cover views, straightforward collection management, and useful progress/error states. Common tasks should require little instruction or configuration. Keep advanced complexity out of the way.

## Technical direction

- **Rust** for the application backend, **Tauri** for the desktop shell, and **React + TypeScript** for the UI (unless investigation reveals a compelling alternative).
- Design for cross-platform use, developing for **macOS first**.
- Prefer existing reliable libraries/tools for EPUB manipulation and conversion rather than recreating ebook engines.
- Keep device-specific behaviour isolated so Kobo and Kindle can support different capabilities.
- Aim for a standalone app without a calibre installation requirement.

Leave specific dependencies, architecture, and implementation details to the engineering agent.

## Reliability

Device data and reading progress must be protected. Handle duplicates, disconnections, failed transfers, partial synchronization, and database changes safely. Back up device state before risky writes. Use Rust's compile-time checks and focused tests for formats, synchronization, and data integrity; validate important operations on real hardware.

## Priorities

1. **Kobo MVP:** Import → cover/metadata editing → KEPUB conversion → transfer → native collection management.
2. **Kindle support:** Transfer/conversion, library browsing, and reliably accessible reading data, while acknowledging collection restrictions.
3. **Later:** Reading analytics, advanced automatic organization, more devices, and cross-platform polish.

**Out of scope:** ebook piracy/download sourcing, DRM circumvention, an ebook store, a full ebook reader/editor, or recreating calibre's advanced feature set.
