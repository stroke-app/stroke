### Bug Fixes
#### Large values
- **Cell panel** - A JSON value of a megabyte or more (a jsonb holding a file) no longer goes blank when scrolled sideways. JSON is laid out with short lines at any size: one key per line, runs of numbers or strings packed into lines.
- **Record view** - A value the grid fetched as a size has a Load button, and shows what it loads.
- **Record view and row inspector** - Large values are shown cut and are edited in the cell panel (Space) instead of a field that could not draw them.
- **Text view** - Very long cells are cut in the preview only. Copy and Download keep them whole.
- **Console** - A new result starts at the top instead of where the last one was scrolled.
