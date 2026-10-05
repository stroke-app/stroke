### Changes

#### AI Assistant
- **Lighter inline code** - Table and column names in a reply are set in mono on a faint tint, at the text's own weight, instead of a bordered chip that read like a key; long inline code wraps instead of running past the message
- **Shortcuts as keys** - A shortcut the assistant writes in backticks (`Ctrl+Shift+P`, `⌘K`, `Esc`) renders as key caps that follow the chat's text size
- The time under a message lines up with the reply's first letter, and the copy and reply buttons no longer spill into the message above; list bullets are quieter than the words they introduce

### Bug Fixes
- **Stop and Escape always end the assistant's turn** - Stop could do nothing, leaving the spinner up with no way out: after a run from a reply's code block, or when a stopped turn that was still settling cleared the next turn's controller as it finished. A stopped turn also kept writing into the next one, and a reply stopped between a tool call and its result broke every later message in the chat
- **The assistant's replies can't take over the window** - HTML in a reply rendered as real elements, so a `<style>` block or a full-window element in a mock-up restyled or covered the app. Raw HTML now shows as text (simple tags like `<br>` and `<b>` still format), and links open only web and mail addresses
