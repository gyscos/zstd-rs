"""Converts a Stack Exchange dump table (one XML element per row, with the
columns as attributes) to JSON lines, one row per line.

usage: python3 se2jsonl.py <Table.xml> <output.jsonl>
"""
import json
import sys
import xml.etree.ElementTree as ET

source, destination = sys.argv[1], sys.argv[2]
rows = 0
with open(destination, "w", encoding="utf-8") as out:
    for _, element in ET.iterparse(source, events=("end",)):
        if element.tag == "row":
            out.write(json.dumps(element.attrib, ensure_ascii=False, separators=(",", ":")))
            out.write("\n")
            rows += 1
        element.clear()
print(f"{destination}: {rows} rows")
