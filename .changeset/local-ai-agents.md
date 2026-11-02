### New Features
- Claude Code, Codex and OpenCode run as AI models when their CLI is installed, using Stroke's database tools only
- Four access levels for agents: read only, read & write, modify schema, dangerous
- Effort and access chips in the AI composer
- Messages sent while the AI is answering are queued
- Ctrl+B toggles the AI chat list, and the list is resizable

### Bug Fixes
- The same AI model no longer shows up twice in the picker
- The sidebar assistant works with local agent models
- get_database_stats works on Postgres
- Rows no longer show through the Related panel's header
- The JSON copy button shows only on the hovered line
- No gap after the row range in the table toolbar

### Changes
- New model picker with provider tabs, search and favourites
- Rebuilt AI chat: tool steps fold into one "Worked for" line, each answer anchors under its message, header buttons float
- Chat titles skip greetings like "hi"
- Slash commands fill the sidebar box instead of sending
- Settings fades in, and dialogs no longer lock page scroll
- Clearer status bar
- "About Stroke" is now "About and license"
