### Bug Fixes

#### Settings
- **No System Events prompt on macOS** - Opening Settings no longer asks to let Stroke control System Events. Launch at login now uses a LaunchAgent file instead of an AppleScript login item, so it needs no permission. Re-enable the switch once if it was on before.
