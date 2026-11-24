### Changes

- **Less style work on every change** - Linux no longer re-checks a whole-document selector on every DOM change to hide scrollbars behind an open dialog; an attribute on the window root does it instead. Text uses the browser's default rendering mode, which keeps WebKit's fast text path while kerning stays on.
