# WP-13 fixtures

`x64-pe.hex` is a wholly fabricated 1024-byte PE32+ AMD64 image, with a DOS
header, one executable `.text` section, and no hardware identifiers. It is
parser input only and must never be executed. Each line contains 32 bytes
encoded as hexadecimal. The unit checks mutate its headers to exercise
architecture, truncation, alignment, section, and directory validation.

The ignored read-only checks in `update.rs` and `win/http.rs` use the real
WinHTTP/CNG APIs. They never run an installer or the elevated application.
