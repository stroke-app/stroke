### Bug Fixes

- **Smooth panel resizing** - Dragging the terminal dock, sidebars or a split pane no longer refits the terminal and reallocates the data grid on every frame. The terminal holds its grid during the drag and refits once on release, and window resizes fit once the drag settles. The terminal dock has a proper handle with a wider grab area, and double-clicking it resets the height.
