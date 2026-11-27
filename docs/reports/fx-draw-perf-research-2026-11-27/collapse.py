"""Collapse perf script records without splitting Rust return arrows/generics."""
import collections
import re
import sys

collapsed = collections.Counter()
for group in open(sys.argv[1]).read().strip().split('\n\n'):
    lines = group.splitlines()
    match = re.search(r':\s+(\d+)\s+\S+:\s*$', lines[0])
    if not match:
        continue
    stack = []
    for line in lines[1:]:
        frame = re.match(r'\s*[0-9a-f]+\s+(.+)\s+\([^()]*\)\s*$', line)
        if frame:
            symbol = re.sub(r'\+0x[0-9a-f]+$', '', frame[1]).replace(';', ',')
            stack.append(symbol)
    key = ';'.join(['composed_streams'] + list(reversed(stack)))
    collapsed[key] += int(match[1])
for key, count in sorted(collapsed.items()):
    print(key, count)
