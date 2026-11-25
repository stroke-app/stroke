### Bug Fixes

#### Interface
- **Smoother window zoom and resize** - Double-clicking the title bar on macOS, maximizing, or dragging the window edge no longer reallocates the data grid's canvas and refits the terminal on every frame. The grid grows its canvas in steps and the terminal refits once the window settles, the way panel drags already worked.
