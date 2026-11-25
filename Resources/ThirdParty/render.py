"""Writes the licenses of the Rust crates an app links, from cargo-about's JSON for each of its builds, one text for
each license however many crates share it. Snowhush's own crates are left out: they are under its own license."""

import json
import sys


def main(out, title, *reports):
    texts = {}
    for report in reports:
        for license in json.load(open(report))["licenses"]:
            users = {f'{use["crate"]["name"]} {use["crate"]["version"]}' for use in license["used_by"]}
            users = {user for user in users if not user.startswith("snowhush")}
            if not users:
                continue
            text = license["text"].strip("\n")
            name, used = texts.setdefault(text, (license["name"], set()))
            used |= users
    with open(out, "w") as file:
        file.write(f"{title}\n\n")
        for text, (name, used) in sorted(texts.items(), key=lambda item: (item[1][0], sorted(item[1][1]))):
            file.write("-" * 100 + "\n")
            file.write(f"{name}, used by {', '.join(sorted(used))}\n\n{text}\n\n")


if __name__ == "__main__":
    main(*sys.argv[1:])
