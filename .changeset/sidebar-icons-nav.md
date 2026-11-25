### Bug Fixes

#### Sidebar
- **Table icons line up with their names** - The icon box stayed 14px while the icon inside grew to 19px, so every table icon sat a few pixels low. The box now matches the icon.
- **Name tooltips only when the name is cut off** - Hovering a table showed its full name below the row, covering the next one, even when nothing was hidden. It now shows only for a truncated name (or one with a comment), beside the row.

#### Status bar
- **Jump buttons are back** - Go to top, go to bottom, first column and last column sit in the status bar again on table tabs. They were dropped with the quieter status bar in #133.
