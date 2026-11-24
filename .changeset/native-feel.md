### Changes

#### Look and feel
- **Desktop density** - Text, icons and bars are smaller and tighter, closer to native desktop apps. The base text size is 14px instead of 16px, icons render at their real 12, 14 and 16px sizes, corners are tighter, and the macOS title bar is 38px (36px on Windows and Linux).
- **Quieter motion** - Buttons no longer shrink when pressed, dialogs fade in instead of zooming, tooltips lose the arrow and bounce, and the sidebar loader is a spinner.
- **System font and scrolling by default** - Unless changed in Settings, Stroke uses the system UI font with the OS's own text smoothing, and native scrolling on every platform. Trackpads always scroll natively, even with eased mouse-wheel scrolling turned on.

#### Window
- **Window size and position are remembered** - Stroke reopens where it was, at the same size, instead of opening maximized every time.
- **Native window chrome** - Windows 11 keeps its window shadow and rounded corners, uses Mica for the glass effect, and gets Fluent caption buttons. Linux gets round GTK-style window buttons and a 1px border on unmaximized windows. On macOS the glass dims with the window when another app has focus, like Finder.
- **Unfocused window** - The title bar and sidebar dim slightly when Stroke isn't the active window.
- **Menu mnemonics** - On Windows and Linux, Alt+F, E, V, T and H open the menus, and holding Alt underlines their letters.
- **Linux GPU rendering** - Stroke now uses GPU compositing on Linux, and falls back to software rendering only after a launch that failed to draw or when `WEBKIT_DISABLE_DMABUF_RENDERER=1` is set.
