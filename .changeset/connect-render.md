### Changes
#### Performance
- **Launch** - The first screen paints about 13% sooner: the AI client and its prompts no longer load with the app.
- **After connecting** - A hidden terminal no longer re-measures itself on every theme or zoom change (it was the largest main-thread cost after a connect), and the sidebar, tab strip and toolbar no longer force the page to lay out in the middle of rendering. The connections list and the tab strip use one right-click menu each instead of one per row.
