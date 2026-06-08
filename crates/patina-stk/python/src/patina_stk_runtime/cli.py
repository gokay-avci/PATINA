from __future__ import annotations

import argparse
from pathlib import Path
import sys

from .contracts import (
    CanonicalizeSmilesRequest,
    DetectFunctionalGroupsRequest,
    EmbedConformerRequest,
    load_json_input,
    write_status,
)
from .runtime import canonicalize_smiles, detect_functional_groups, embed_conformer, toolkit_status


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="patina-stk-runtime",
        description="Run isolated RDKit-backed adapter commands for patina-stk.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    status = subparsers.add_parser(
        "status",
        help="Report the active Python and scientific toolkit versions.",
    )
    status.add_argument("--response", type=Path, required=False)

    subparsers.add_parser(
        "canonicalize-smiles",
        help="Canonicalize a SMILES string from a JSON stdin request.",
    )
    subparsers.add_parser(
        "detect-functional-groups",
        help="Run SMARTS-based functional-group detection from a JSON stdin request.",
    )
    subparsers.add_parser(
        "embed-conformer",
        help="Embed a 3D conformer from a JSON stdin request.",
    )
    return parser


def main() -> int:
    parser = build_parser()
    args = parser.parse_args()

    if args.command == "status":
        status = toolkit_status()
        if args.response is not None:
            write_status(args.response, status)
        else:
            print(status.to_json())
        return 0

    if args.command == "canonicalize-smiles":
        request = CanonicalizeSmilesRequest.from_mapping(load_json_input())
        response = canonicalize_smiles(request)
        sys.stdout.write(response.to_json() + "\n")
        return 0

    if args.command == "detect-functional-groups":
        request = DetectFunctionalGroupsRequest.from_mapping(load_json_input())
        response = detect_functional_groups(request)
        sys.stdout.write(response.to_json() + "\n")
        return 0

    if args.command == "embed-conformer":
        request = EmbedConformerRequest.from_mapping(load_json_input())
        response = embed_conformer(request)
        sys.stdout.write(response.to_json() + "\n")
        return 0

    parser.error(f"unsupported command: {args.command}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
