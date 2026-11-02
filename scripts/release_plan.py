#!/usr/bin/env python3
"""Plan GitHub releases without publishing packages or changing source versions."""

import json
import os
import re
import tomllib
from pathlib import Path

VERSION = r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)"


def release_plan(version: str, event: str, ref: str, run_number: str) -> dict[str, str]:
    if not re.fullmatch(VERSION, version):
        raise ValueError(f"Expected a plain workspace version, got {version!r}")
    if not re.fullmatch(r"[1-9][0-9]*", run_number):
        raise ValueError("Expected a positive GitHub run number")

    tag = ""
    beta = False
    publish = False
    if event == "workflow_dispatch":
        # Manual runs remain artifact-only, even when dispatched against a tag.
        pass
    elif event == "push" and ref == "refs/heads/main":
        tag = f"v{version}-beta.{run_number}"
        beta = True
        publish = True
    elif event == "push" and ref.startswith("refs/tags/"):
        tag = ref.removeprefix("refs/tags/")
        beta = bool(re.fullmatch(rf"v{re.escape(version)}-beta\.[1-9][0-9]*", tag))
        if tag != f"v{version}" and not beta:
            raise ValueError(f"Tag {tag!r} does not match workspace version {version}")
        publish = True
    else:
        raise ValueError(f"Unsupported release event/ref: {event!r}, {ref!r}")

    return {
        "version": version,
        "tag": tag,
        "prerelease": str(beta).lower(),
        "publish": str(publish).lower(),
        "production_signing": str(publish and not beta).lower(),
    }


def main() -> None:
    with Path("Cargo.toml").open("rb") as manifest:
        version = tomllib.load(manifest)["workspace"]["package"]["version"]
    plan = release_plan(
        version,
        os.environ["GITHUB_EVENT_NAME"],
        os.environ["GITHUB_REF"],
        os.environ["GITHUB_RUN_NUMBER"],
    )
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        for key, value in plan.items():
            output.write(f"{key}={value}\n")
    print(json.dumps(plan, indent=2))


if __name__ == "__main__":
    main()
