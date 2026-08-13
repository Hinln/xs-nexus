#!/usr/bin/env python3

import copy
import json
import subprocess
import sys
import tempfile
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
VALIDATOR = ROOT / "scripts" / "validate-nft-gateway-rules.py"


def run(document: dict, expected_success: bool) -> None:
    with tempfile.TemporaryDirectory() as temporary:
        fixture = Path(temporary) / "rules.json"
        fixture.write_text(json.dumps(document), encoding="utf-8")
        result = subprocess.run(
            [
                sys.executable,
                str(VALIDATOR),
                "--input",
                str(fixture),
                "--table",
                "xs_nexus_xsb0",
                "--chain",
                "postrouting",
                "--input-interface",
                "xsb0",
                "--output-interface",
                "xsm32lanb",
                "--destination",
                "192.168.232.0/24",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        if (result.returncode == 0) != expected_success:
            raise AssertionError(result.stdout + result.stderr)


def rule(destination: object) -> dict:
    return {
        "nftables": [
            {"metainfo": {"json_schema_version": 1}},
            {"table": {"family": "ip", "name": "xs_nexus_xsb0"}},
            {
                "chain": {
                    "family": "ip",
                    "table": "xs_nexus_xsb0",
                    "name": "postrouting",
                    "type": "nat",
                    "hook": "postrouting",
                    "prio": 100,
                }
            },
            {
                "rule": {
                    "family": "ip",
                    "table": "xs_nexus_xsb0",
                    "chain": "postrouting",
                    "expr": [
                        {
                            "match": {
                                "op": "==",
                                "left": {"meta": {"key": "iifname"}},
                                "right": "xsb0",
                            }
                        },
                        {
                            "match": {
                                "op": "==",
                                "left": {"meta": {"key": "oifname"}},
                                "right": "xsm32lanb",
                            }
                        },
                        {
                            "match": {
                                "op": "==",
                                "left": {"payload": {"protocol": "ip", "field": "daddr"}},
                                "right": destination,
                            }
                        },
                        {"masquerade": None},
                    ],
                }
            },
        ]
    }


direct = rule("192.168.232.0/24")
run(direct, True)
run(rule({"prefix": {"addr": "192.168.232.0", "len": 24}}), True)
masked = rule("192.168.232.0")
masked["nftables"][3]["rule"]["expr"][2]["match"]["left"] = {
    "&": [
        {"payload": {"protocol": "ip", "field": "daddr"}},
        "255.255.255.0",
    ]
}
run(masked, True)

for mutation in ("input", "output", "destination", "masquerade", "chain"):
    invalid = copy.deepcopy(direct)
    current_rule = invalid["nftables"][3]["rule"]
    if mutation == "input":
        current_rule["expr"][0]["match"]["right"] = "wrong0"
    elif mutation == "output":
        current_rule["expr"][1]["match"]["right"] = "wrong1"
    elif mutation == "destination":
        current_rule["expr"][2]["match"]["right"] = "192.168.233.0/24"
    elif mutation == "masquerade":
        current_rule["expr"].pop()
    else:
        current_rule["chain"] = "input"
    run(invalid, False)

invalid_table = copy.deepcopy(direct)
invalid_table["nftables"][1]["table"]["name"] = "other"
run(invalid_table, False)
invalid_chain = copy.deepcopy(direct)
invalid_chain["nftables"][2]["chain"]["type"] = "filter"
run(invalid_chain, False)

print("nft gateway validator regression tests passed")
