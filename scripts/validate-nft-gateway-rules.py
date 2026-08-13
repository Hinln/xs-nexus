#!/usr/bin/env python3

import argparse
import ipaddress
import json
from pathlib import Path
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--table", required=True)
    parser.add_argument("--chain", required=True)
    parser.add_argument("--input-interface", required=True)
    parser.add_argument("--output-interface", required=True)
    parser.add_argument("--destination", type=ipaddress.ip_network, required=True)
    return parser.parse_args()


def contains(value: Any, expected: Any) -> bool:
    if value == expected:
        return True
    if isinstance(value, dict):
        return any(contains(item, expected) for item in value.values())
    if isinstance(value, list):
        return any(contains(item, expected) for item in value)
    return False


def is_interface_match(expression: Any, key: str, interface: str) -> bool:
    if not isinstance(expression, dict):
        return False
    match = expression.get("match")
    if not isinstance(match, dict) or match.get("op") != "==":
        return False
    left = match.get("left")
    return contains(left, {"meta": {"key": key}}) and match.get("right") == interface


def is_destination_match(expression: Any, destination: ipaddress._BaseNetwork) -> bool:
    if not isinstance(expression, dict):
        return False
    match = expression.get("match")
    if not isinstance(match, dict) or match.get("op") != "==":
        return False
    left = match.get("left")
    if not contains(left, {"payload": {"protocol": "ip", "field": "daddr"}}):
        return False
    right = match.get("right")
    direct_values = {
        str(destination),
        str(destination.network_address),
    }
    if isinstance(right, str) and right in direct_values:
        if right == str(destination):
            return True
        return contains(left, str(destination.netmask))
    return right == {
        "prefix": {
            "addr": str(destination.network_address),
            "len": destination.prefixlen,
        }
    }


def is_masquerade(expression: Any) -> bool:
    return isinstance(expression, dict) and "masquerade" in expression


def validates_rule(rule: dict[str, Any], args: argparse.Namespace) -> bool:
    if rule.get("family") != "ip" or rule.get("table") != args.table:
        return False
    if rule.get("chain") != args.chain:
        return False
    expressions = rule.get("expr")
    if not isinstance(expressions, list):
        return False
    return all(
        (
            any(is_interface_match(item, "iifname", args.input_interface) for item in expressions),
            any(is_interface_match(item, "oifname", args.output_interface) for item in expressions),
            any(is_destination_match(item, args.destination) for item in expressions),
            any(is_masquerade(item) for item in expressions),
        )
    )


def validates_table(table: Any, args: argparse.Namespace) -> bool:
    return (
        isinstance(table, dict)
        and table.get("family") == "ip"
        and table.get("name") == args.table
    )


def validates_chain(chain: Any, args: argparse.Namespace) -> bool:
    return (
        isinstance(chain, dict)
        and chain.get("family") == "ip"
        and chain.get("table") == args.table
        and chain.get("name") == args.chain
        and chain.get("type") == "nat"
        and chain.get("hook") == "postrouting"
        and chain.get("prio") in (100, "srcnat")
    )


def main() -> None:
    args = parse_args()
    document = json.loads(args.input.read_text(encoding="utf-8"))
    objects = document.get("nftables")
    if not isinstance(objects, list):
        raise SystemExit("nft JSON has no nftables object list")
    tables = [item.get("table") for item in objects if isinstance(item, dict)]
    if not any(validates_table(table, args) for table in tables):
        raise SystemExit("required project NAT table is absent")
    chains = [item.get("chain") for item in objects if isinstance(item, dict)]
    if not any(validates_chain(chain, args) for chain in chains):
        raise SystemExit("required NAT postrouting chain is absent")
    rules = [item.get("rule") for item in objects if isinstance(item, dict)]
    if not any(isinstance(rule, dict) and validates_rule(rule, args) for rule in rules):
        raise SystemExit("required scoped gateway NAT rule is absent")
    print("scoped gateway NAT rule validation passed")


if __name__ == "__main__":
    main()
