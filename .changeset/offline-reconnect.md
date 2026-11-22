### Bug Fixes
- **Reconnecting while offline** - With no network, the reconnect on launch no longer dials into nothing and then blames the host. It says you're offline and reconnects by itself when the network is back. A failed connect while offline says so, and repeated failures show one toast instead of a stack.
