---
"stroke": patch
---

### Bug Fixes
- The sign-in screen could appear after an update or a restart when the system keyring answered slowly at launch. The app now retries the keyring and stays signed in on its signed account cache meanwhile.
- With the table search focused, clicking the filter, sort or column icons beside it often missed: the field shrank on blur and the icons slid under the pointer. The field no longer changes width on focus.
- Scrolling a large table into rows still loading stuttered while those rows faded in. They now appear in one frame.
- A toast with a long message squeezed its text into a narrow column beside its button; the button now goes under the text. Connection errors written as plain sentences are no longer set in monospace.

### Changes
- The flat layout is back as the default. The inset content panel is still in Settings, Appearance.
- Geist is the default font on Linux.
- Focused fields show a firmer 2px accent edge.
- The status bar is shorter, the Live toggle is a plain status item, the Linux title bar is 32px, and the close-with-unsaved-changes dialog uses the app's own buttons.
