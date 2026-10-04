# WP-04 formatting fixtures

All device identifiers in these fixtures are fabricated. They exercise RAM table
UTF-16 widths, untrimmed strings, null values, capacity grouping, CPU null versus
empty versus OEM-placeholder serials, and a nonzero CPUID leaf-3 serial with the
high bit set. The intentionally oversized RAM capacity exercises invariant N0
grouping and is not a hardware claim.

The expected text was generated from the historical C# formatting rules on .NET 10.
JSON string fixtures preserve explicit CRLF escapes and trailing spaces despite
checkout line-ending settings; the decoded text is checked byte for byte. Real captures
belong only in the git-ignored main-checkout golden/wp-04 directory.
