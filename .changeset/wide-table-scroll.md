### Bug Fixes
- **Skeleton rows on wide tables over a slow link** - On a big table with wide rows (HTML, JSON), the rows you scroll or jump to now show in about a second instead of after several, and scrolling sideways no longer sits on skeletons. The rows on screen are fetched first, and the background copy of the table yields to them.
