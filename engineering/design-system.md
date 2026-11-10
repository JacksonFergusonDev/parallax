# Parallax design system

Parallax is a quiet, precise desktop library and reader. The name connects different viewpoints on one collection: the library, the reader, and the device. Warm paper, ink, and a restrained gold accent frame the books rather than compete with them. This document is the UI contract for new work, alongside `app/src/design/tokens.css`.

## Identity

Use **Parallax** as the product name in window titles, UI copy, packaging, and current documentation. The canonical mark is `app/src-tauri/icons/source/icon.svg`: two offset observing arcs and converging sight lines share a luminous reference point. Its navy rounded square (`#0f172a`), gold strokes (`#f5c568`), and white core belong to the same geometric family as the owner's other products. This is a symbolic mark, not an astrophysical diagram.

Keep the icon's proportions, padding, colors, and rounded container intact. Do not add gradients, shadows, lettering, or a second accent inside it. Use the full icon at 32 logical pixels or larger in the UI; the generated 16-pixel ICO representation is reserved for system surfaces. The wordmark uses DM Sans at the label size and medium/semibold weight. The header may hide the wordmark at narrow widths, retaining its accessible name. Avoid repeating the logo inside working panels.

The web copy is `app/public/parallax.svg`. Keep the SVG copies identical. The desktop raster has transparent padding so the rounded square fills about 83% of the Dock icon slot. From the repository root, regenerate it with `inkscape app/src-tauri/icons/source/icon.svg --export-area=-19:-19:219:219 --export-width=1024 --export-height=1024 --export-filename=app/src-tauri/icons/source/icon.png`, then run `pnpm --dir app tauri icon src-tauri/icons/source/icon.png`. Keep the desktop outputs.

Use `Parallax` for the desktop executable so macOS shows the product name when running it directly during development. Use `dev.parallax.app` for the desktop bundle identifier, `parallax-cli` for the CLI, `parallax-*` for workspace packages, and `.parallax` for Parallax records on Kobo devices.

The desktop and CLI executable names must differ beyond letter case so both can build into the same directory on macOS.

## Typography and density

Use the shared semantic tokens, never a new literal UI font size. At the default 14-pixel root, the authored UI range is 13–24 logical/CSS pixels. This bounds design choices, not accessibility zoom. Relative type tokens scale with the root; minimum control heights permit controls to grow with text. Do not shrink text to fit a window.

| Role | Token | Default size | Use |
| --- | --- | --- | --- |
| Supporting | `--text-small` | 13 px | Secondary metadata, hints, table cells |
| Body | `--text-body` | 14 px | Controls, inputs, ordinary content |
| Label | `--text-label` | 16 px | Prominent labels and panel headings |
| Heading | `--text-heading` | 20 px | Section headings |
| Display | `--text-display` | 24 px | Reading headline, primary dialog title, statistics |

Use DM Sans (`--font-ui`) for controls, metadata, and instructions. It is the house typeface shared with the Parallax site and the owner's other sites, self-hosted from the pinned [house-style](https://github.com/JacksonFergusonDev/house-style) release in `app/package.json`; only `css/fonts.css` is imported, since the app keeps its own palette, tokens and themes. Its files cover Latin only, so other scripts in book metadata fall back to the system font glyph by glyph. Reserve Fraunces for book titles, the reading headline, and reading statistics; it should not decorate operational controls. Use weight and spacing before adding another size. Body line height is 1.45; controls use 1.4. Truncate long labels only where the full value is available through the detail view or an accessible title.

There are two explicit exceptions: publisher content in the reader has its own user-adjustable 80–200% scale, and generated cover lettering is artwork that scales with a cover. The `--art-type-*` tokens are restricted to the generated-cover selectors listed in the design contract test; they must never be used for interface labels. Book titles and authors also appear outside the cover at readable UI sizes.

## Dimensions and rhythm

The default desktop density is comfortable and consistent. Standard buttons, selects, and text inputs have a 36 px minimum height. Compact toolbar/menu buttons and icon targets have a 32 px minimum. Use 16 px icons, or 20 px for a more prominent icon, with a consistent stroke weight. A small icon must not imply a small hit target. Checkboxes are 16 px and should have a clickable text label.

Use `--space-1/2/3/4/6/8` for the 4, 8, 12, 16, 24, and 32 px spacing scale. Control radius is `--radius-small` (6 px), panels use `--radius` (10 px), and dialogs use `--radius-large` (14 px). Pills, charts, book silhouettes, and cover art have geometric exceptions; those are not alternative control styles. Book dimensions, chart geometry, and responsive constraints need not be multiples of the spacing scale.

The initial main window is 1280 × 900, with an 800 × 560 minimum. Existing saved window sizes remain valid. The detail panel is 320 px and the collections sidebar 224 px at normal width. At 1100 px and below, the header wraps search to its own row, the wordmark hides, and panels use narrower layouts. At 900 px and below, the empty detail placeholder hides. Active details remain reachable and scroll independently. Toolbars, filters, and footer actions wrap; they must not reduce font sizes. Dialogs stay within the viewport and scroll when their content cannot fit.

## Color and interaction

Every window imports `tokens.css`. Use semantic roles (`--bg`, `--surface`, `--sunken`, `--fg`, `--muted`, `--border`, `--accent`, `--accent-fg`, `--ok`, `--warn`, `--danger`, `--status-fg`), not isolated hex colors in new controls. Gold is darker in light mode so text and primary buttons remain legible; dark mode uses luminous gold against cool ink surfaces. Error and warning messages must include text or an icon, not color alone. Book artwork and charts can have their own palettes.

Normal UI text and semantic status colors must maintain at least 4.5:1 contrast on their base surfaces. The palette test checks this in both themes, including primary button text and text on solid status badges. Color mixes, opacity, overlaid cover badges, and focus rings still require visual review; passing the base palette test is not a complete accessibility audit. Avoid reducing the opacity of readable metadata.

Shared control primitives live in `tokens.css`; main-window component styles live in `styles.css`, and reader-specific layout lives in `reader/reader.css`. Reader page palettes are in `reader/appearance.ts` because publisher content runs in isolated documents. Its paper, ink, and link colors track each application theme (see Themes).

Use visible keyboard focus, consistent hover/pressed/disabled states, and a stable primary action location. Standard UI feedback uses the 120 ms or 200 ms motion tokens. Existing cover-lift motion is a deliberate artwork exception. Respect reduced motion in both windows. Preserve the existing focus, keyboard shortcuts, selection, and safe device-operation behavior when changing presentation.

## Themes

The Settings window offers System, Light, Dark, Sepia and High contrast. Each window shows one palette through `data-theme` on its root (`light`, `dark`, `sepia`, `contrast` or `contrast-dark`), chosen by `app/src/design/theme.ts`: System and High contrast follow macOS between light and dark. Every palette in `tokens.css` defines the same colour roles, and a `[data-theme]` element anywhere previews that palette, which is how the Settings window draws its theme swatches. Before a window sets its theme, `:root:not([data-theme])` follows the system; the contract test keeps that fallback identical to Dark.

Sepia is warm paper for long reading, built around the reader's sepia page. High contrast keeps every text colour, status colour and primary button at 7:1 or better and uses stronger borders; the palette test enforces it. The gold accent stays in every theme. Reader page palettes in `reader/appearance.ts` mirror each theme (the page is the surface in light themes, the background in dark ones), and a reader on Auto follows the app. New UI must work in all five palettes: use the colour roles, never a colour chosen for one theme.

## Verification and future changes

`app/src/design/design.test.ts` scans authored CSS for unapproved type sizes, restricts artwork exceptions, rejects alternative common control radii, verifies the type range, and checks palette contrast. It runs through the ordinary `pnpm test` / `just check` workflow. New tokens need a role and a corresponding update here; do not add a token merely to preserve an arbitrary local value. Inline font sizes and local font shorthands are not permitted for new UI.

Before accepting visual changes, inspect Library (covers and list), selected book details, On Kobo, Reading, menus, a large review dialog, and the reader. Check every theme (at least Light, Dark and High contrast), the default and minimum window sizes, long metadata, increased text scale, keyboard focus, and reduced motion. Browser mock data is appropriate for presentation checks; it does not verify actual hardware behavior.
