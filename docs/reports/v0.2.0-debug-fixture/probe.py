import json, sys
items = json.loads(sys.argv[1])
print(json.dumps([round(item["amount"] * (1 + (item.get("rate") or 0.10)), 2) for item in items]))
