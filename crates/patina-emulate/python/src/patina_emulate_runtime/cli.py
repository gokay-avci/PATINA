from __future__ import annotations

import argparse
import os
from pathlib import Path

from .contracts import load_request, write_response
from .runtime import score_candidates
from .soap_features import load_feature_request, project_features, write_feature_response


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="patina-emulate-runtime",
        description="Run isolated AutoEmulate/SVGP scoring adapters for patina-emulate.",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    score = subparsers.add_parser(
        "score-candidates",
        help="Fit the configured surrogate and rank pending candidates for one branch.",
    )
    score.add_argument("--request", type=Path, required=True)
    score.add_argument("--response", type=Path, required=True)

    features = subparsers.add_parser(
        "project-features",
        help="Project structures into DScribe or featomic SOAP descriptor vectors.",
    )
    features.add_argument("--request", type=Path, required=True)
    features.add_argument("--response", type=Path, required=True)
    return parser


def main() -> int:
    project_root = Path(__file__).resolve().parents[2]
    os.environ.setdefault("MPLCONFIGDIR", str(project_root / ".mplcache"))

    parser = build_parser()
    args = parser.parse_args()

    if args.command == "score-candidates":
        request = load_request(args.request)
        response = score_candidates(request)
        write_response(args.response, response)
        return 0

    if args.command == "project-features":
        request = load_feature_request(args.request)
        response = project_features(request)
        write_feature_response(args.response, response)
        return 0

    parser.error(f"unsupported command: {args.command}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
