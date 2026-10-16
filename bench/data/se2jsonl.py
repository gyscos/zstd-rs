"""Converts a Stack Exchange dump table (one XML element per row, with the
columns as attributes) to JSON lines, one row per line.

usage: python3 se2jsonl.py <Table.xml> <output.jsonl>
"""
import json
import sys
import xml.etree.ElementTree as ET

source, destination = sys.argv[1], sys.argv[2]
rows = 0
root = None
with open(destination, "w", encoding="utf-8") as out:
    for event, element in ET.iterparse(source, events=("start", "end")):
        if root is None:
            root = element
        if event == "end" and element.tag == "row":
            out.write(json.dumps(element.attrib, ensure_ascii=False, separators=(",", ":")))
            out.write("\n")
            rows += 1
            # Clearing the row is not enough: the root still holds on to
            # it, and memory would grow with the size of the table.
            root.clear()
print(f"{destination}: {rows} rows")
