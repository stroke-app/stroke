### Changes
#### Performance
- **Jumping deep into a large table** - A jump to rows nothing has loaded near shows them in about a third of the time over a remote link: the rows on screen are fetched first, and a deep page ordered by the primary key skips ahead on the key's index instead of reading every row before it.
- **Local copy of large tables** - Fills about four times faster locally and twice as fast over a far link, and waits until the grid is quiet so it never slows the rows on screen.
