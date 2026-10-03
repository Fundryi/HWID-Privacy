# WP-09 fixtures

All identifiers in these files are fabricated. MACs retain plausible vendor OUI
prefixes; GUIDs and device-instance suffixes identify no captured device. IPs use
documentation ranges (`192.0.2.0/24`, `198.51.100.0/24`, `2001:db8::/32`) and
synthetic link-local suffixes.

- `network-format.json`: C# field/filter examples and all AD-20 label cases;
  includes a mismatched interface GUID and an overlong native MAC length.
- `arp-native-format.json`: C# neighbor formatting, all real neighbor states
  0 through 6, incomplete/zero/broadcast/multicast filtering, both IP families,
  string ordering, unknown names, virtual tags, and an IPv6-index name.
- `arp-exe-format.json`: C# English fallback grammar, case preservation,
  space-only splitting, filtered records, and empty/localized output.

These are source-derived synthetic format/parser cases, not scrubbed native
captures. Native capture provenance remains blocked on the missing safe IpHelper
wrappers. The non-admin provider capture stays outside git under the main
checkout's `app/rust/golden/wp-09/`.
