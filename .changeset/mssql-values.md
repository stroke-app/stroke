### Bug Fixes
- **SQL Server values** - decimal, numeric, uniqueidentifier and datetimeoffset columns showed NULL; they show their values, decimals at their exact scale. A datetime shows .997 instead of .996666666.
- **SQL Server types** - Column headers say int, bit and nvarchar instead of int4, intn and bitn.
