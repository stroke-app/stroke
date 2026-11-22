### Changes

#### Performance
- **Tables with large JSON or text** - Values past 8 KB arrive as their first 512 characters and their size, and load in full when the cell is opened or edited. A page of rows holding ~600 KB documents is a third of the size it was, and the grid shows each value's start instead of "[jsonb · 15 KB, truncated]".
