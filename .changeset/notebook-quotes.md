### Bug Fixes
- **Notebook variables** - A value with an apostrophe (O'Neil) inside `'{{name}}'` no longer breaks the cell or lets the value write SQL: its quotes are doubled inside a string literal.
