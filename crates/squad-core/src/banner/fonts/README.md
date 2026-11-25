# Embedded fonts

`standard.flf` and `small.flf` are unmodified copies from figlet-rs 1.0.0's
`resources/` directory. Standard is the default; Small is the compact fallback.
The original FIGlet distribution is BSD-3-Clause:
https://github.com/cmatsuoka/figlet/blob/master/LICENSE

See LICENSE-FIGlet.txt and each font's original header notices. No fonts are
loaded from disk at runtime: `FIGlet::from_content(include_str!(...))` parses
the embedded UTF-8 data once. Font output and selection are independent of the
environment; only the TUI's explicit width and mode affect selection.
