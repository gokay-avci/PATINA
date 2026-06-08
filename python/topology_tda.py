#!/usr/bin/env python3
"""Persistent-homology sidecar for PATINA topology candidates.

This script is intentionally optional. It reads the Rust topology-library JSONL
format and writes TDA descriptors as JSONL so the core Rust crate does not
depend on Python, GUDHI, or plotting libraries.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable

import gudhi
import numpy as np


@dataclass(frozen=True)
class Candidate:
    candidate_id: str
    generator: str
    formula: str
    positions: np.ndarray
    bonds: list[tuple[int, int, float]]


def load_candidates(path: Path) -> list[Candidate]:
    candidates: list[Candidate] = []
    with path.open() as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            raw = json.loads(line)
            atoms = raw["atoms"]
            positions = np.array([atom["position"] for atom in atoms], dtype=float)
            bonds = [
                (int(bond["i"]), int(bond["j"]), float(bond.get("distance", 1.0)))
                for bond in raw["bonds"]
            ]
            if positions.ndim != 2 or positions.shape[1] != 3:
                raise ValueError(f"{path}:{line_number}: expected 3D positions")
            candidates.append(
                Candidate(
                    candidate_id=raw["id"]["0"] if isinstance(raw["id"], dict) else raw["id"],
                    generator=raw["generator"]["name"],
                    formula=raw["composition"]["total_counts"]
                    and raw["composition"].get("total_formula", "")
                    or "",
                    positions=positions,
                    bonds=bonds,
                )
            )
    return candidates


def candidate_formula(candidate: dict[str, Any]) -> str:
    order = candidate["composition"]["formula_unit"]["order"]
    counts = candidate["composition"]["total_counts"]
    out = []
    for element in order:
        count = counts[element]
        out.append(element if count == 1 else f"{element}{count}")
    return "".join(out)


def load_candidates_with_formula(path: Path) -> list[Candidate]:
    candidates: list[Candidate] = []
    with path.open() as handle:
        for line_number, line in enumerate(handle, start=1):
            if not line.strip():
                continue
            raw = json.loads(line)
            atoms = raw["atoms"]
            positions = np.array([atom["position"] for atom in atoms], dtype=float)
            bonds = [
                (int(bond["i"]), int(bond["j"]), float(bond.get("distance", 1.0)))
                for bond in raw["bonds"]
            ]
            if positions.ndim != 2 or positions.shape[1] != 3:
                raise ValueError(f"{path}:{line_number}: expected 3D positions")
            candidates.append(
                Candidate(
                    candidate_id=raw["id"],
                    generator=raw["generator"]["name"],
                    formula=candidate_formula(raw),
                    positions=positions,
                    bonds=bonds,
                )
            )
    return candidates


def infer_max_edge_length(candidate: Candidate, multiplier: float) -> float:
    if candidate.bonds:
        distances = sorted(distance for _, _, distance in candidate.bonds)
        median = distances[len(distances) // 2]
        return median * multiplier
    if len(candidate.positions) < 2:
        return 0.0
    distances = []
    for i in range(len(candidate.positions)):
        for j in range(i + 1, len(candidate.positions)):
            distances.append(float(np.linalg.norm(candidate.positions[i] - candidate.positions[j])))
    distances.sort()
    return distances[len(distances) // 2] * multiplier


def graph_distance_matrix(candidate: Candidate, weighted: bool) -> np.ndarray:
    n = len(candidate.positions)
    matrix = np.full((n, n), np.inf, dtype=float)
    np.fill_diagonal(matrix, 0.0)
    for i, j, distance in candidate.bonds:
        weight = distance if weighted else 1.0
        matrix[i, j] = min(matrix[i, j], weight)
        matrix[j, i] = min(matrix[j, i], weight)
    for k in range(n):
        matrix = np.minimum(matrix, matrix[:, [k]] + matrix[[k], :])
    if np.isinf(matrix).any():
        raise ValueError(f"{candidate.candidate_id}: graph-distance filtration requires connected graph")
    return matrix


def persistence_for_candidate(
    candidate: Candidate,
    filtration: str,
    max_dimension: int,
    max_edge_length: float | None,
    graph_weighted: bool,
) -> dict[str, Any]:
    if max_edge_length is None:
        max_edge_length = infer_max_edge_length(candidate, 2.5)
    if filtration == "rips":
        complex_ = gudhi.RipsComplex(points=candidate.positions, max_edge_length=max_edge_length)
    elif filtration == "graph-distance":
        complex_ = gudhi.RipsComplex(
            distance_matrix=graph_distance_matrix(candidate, graph_weighted),
            max_edge_length=max_edge_length,
        )
    else:
        raise ValueError(f"unsupported filtration {filtration!r}")

    simplex_tree = complex_.create_simplex_tree(max_dimension=max_dimension + 1)
    simplex_tree.persistence(homology_coeff_field=2, min_persistence=0.0)
    diagrams = {
        str(dim): intervals_to_json(simplex_tree.persistence_intervals_in_dimension(dim))
        for dim in range(max_dimension + 1)
    }
    summaries = {
        str(dim): summarize_intervals(diagrams[str(dim)])
        for dim in range(max_dimension + 1)
    }
    return {
        "schema_version": "patina-topology-tda/v1",
        "candidate_id": candidate.candidate_id,
        "generator": candidate.generator,
        "formula": candidate.formula,
        "filtration": filtration,
        "max_dimension": max_dimension,
        "max_edge_length": max_edge_length,
        "n_atoms": int(len(candidate.positions)),
        "n_bonds": int(len(candidate.bonds)),
        "simplex_tree": {
            "num_vertices": int(simplex_tree.num_vertices()),
            "num_simplices": int(simplex_tree.num_simplices()),
        },
        "diagrams": diagrams,
        "summaries": summaries,
        "features": flatten_summary_features(summaries),
    }


def intervals_to_json(intervals: np.ndarray) -> list[list[float | None]]:
    out: list[list[float | None]] = []
    for birth, death in intervals.tolist():
        out.append([float(birth), None if math.isinf(float(death)) else float(death)])
    return out


def summarize_intervals(intervals: list[list[float | None]]) -> dict[str, float | int]:
    finite_lifetimes = [
        death - birth
        for birth, death in intervals
        if death is not None and death >= birth
    ]
    infinite_count = sum(1 for _, death in intervals if death is None)
    total = float(sum(finite_lifetimes))
    max_life = float(max(finite_lifetimes)) if finite_lifetimes else 0.0
    mean_life = total / len(finite_lifetimes) if finite_lifetimes else 0.0
    return {
        "interval_count": len(intervals),
        "finite_count": len(finite_lifetimes),
        "infinite_count": infinite_count,
        "total_persistence": total,
        "max_persistence": max_life,
        "mean_persistence": mean_life,
    }


def flatten_summary_features(summaries: dict[str, dict[str, float | int]]) -> dict[str, float | int]:
    features: dict[str, float | int] = {}
    for dim, summary in summaries.items():
        for key, value in summary.items():
            features[f"ph_d{dim}_{key}"] = value
    return features


def write_jsonl(path: Path, records: Iterable[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w") as handle:
        for record in records:
            handle.write(json.dumps(record, sort_keys=True))
            handle.write("\n")


def write_csv(path: Path, records: list[dict[str, Any]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    fieldnames = [
        "candidate_id",
        "generator",
        "formula",
        "filtration",
        "n_atoms",
        "n_bonds",
        "num_simplices",
    ]
    feature_names = sorted({key for record in records for key in record["features"]})
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames + feature_names)
        writer.writeheader()
        for record in records:
            row = {
                "candidate_id": record["candidate_id"],
                "generator": record["generator"],
                "formula": record["formula"],
                "filtration": record["filtration"],
                "n_atoms": record["n_atoms"],
                "n_bonds": record["n_bonds"],
                "num_simplices": record["simplex_tree"]["num_simplices"],
            }
            row.update(record["features"])
            writer.writerow(row)


def finite_diagram(record: dict[str, Any], dimension: int) -> np.ndarray:
    rows = [
        [birth, death]
        for birth, death in record["diagrams"].get(str(dimension), [])
        if death is not None
    ]
    if not rows:
        return np.empty((0, 2), dtype=float)
    return np.array(rows, dtype=float)


def write_similarity_dot(
    path: Path,
    records: list[dict[str, Any]],
    dimension: int,
    threshold: float,
) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    diagrams = [finite_diagram(record, dimension) for record in records]
    with path.open("w") as handle:
        handle.write("graph patina_tda_bottleneck {\n")
        handle.write('  node [shape=circle, style=filled, fontname="Menlo"];\n')
        for record in records:
            label = f"{record['candidate_id']}\\n{record['generator']}"
            handle.write(f'  "{record["candidate_id"]}" [label="{label}", fillcolor="#d8eee8"];\n')
        for i in range(len(records)):
            for j in range(i + 1, len(records)):
                distance = gudhi.bottleneck_distance(diagrams[i], diagrams[j])
                if distance <= threshold:
                    handle.write(
                        f'  "{records[i]["candidate_id"]}" -- "{records[j]["candidate_id"]}" '
                        f'[label="{distance:.3g}"];\n'
                    )
        handle.write("}\n")


def plot_diagrams(records: list[dict[str, Any]], out_dir: Path, limit: int) -> None:
    import matplotlib.pyplot as plt

    out_dir.mkdir(parents=True, exist_ok=True)
    for record in records[:limit]:
        intervals = []
        for dim, diagram in record["diagrams"].items():
            for birth, death in diagram:
                if death is not None:
                    intervals.append((int(dim), (birth, death)))
        if not intervals:
            continue
        ax = gudhi.plot_persistence_diagram(intervals)
        ax.set_title(f"{record['candidate_id']} {record['generator']}")
        plt.tight_layout()
        plt.savefig(out_dir / f"{record['candidate_id']}_persistence.png", dpi=160)
        plt.close()


def cmd_compute(args: argparse.Namespace) -> None:
    candidates = load_candidates_with_formula(args.input)
    records = [
        persistence_for_candidate(
            candidate,
            filtration=args.filtration,
            max_dimension=args.max_dimension,
            max_edge_length=args.max_edge_length,
            graph_weighted=args.graph_weighted,
        )
        for candidate in candidates[: args.limit if args.limit else None]
    ]
    write_jsonl(args.out, records)
    if args.csv_out:
        write_csv(args.csv_out, records)
    if args.plots_dir:
        plot_diagrams(records, args.plots_dir, args.plot_limit)
    print(f"wrote {len(records)} TDA records to {args.out}")


def cmd_similarity(args: argparse.Namespace) -> None:
    records = [json.loads(line) for line in args.input.read_text().splitlines() if line.strip()]
    write_similarity_dot(args.out, records, args.dimension, args.threshold)
    print(f"wrote TDA similarity graph to {args.out}")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    compute = sub.add_parser("compute", help="Compute PH descriptors for candidates.jsonl")
    compute.add_argument("--input", type=Path, required=True)
    compute.add_argument("--out", type=Path, required=True)
    compute.add_argument("--csv-out", type=Path)
    compute.add_argument("--filtration", choices=["rips", "graph-distance"], default="rips")
    compute.add_argument("--max-dimension", type=int, default=2)
    compute.add_argument("--max-edge-length", type=float)
    compute.add_argument("--graph-weighted", action="store_true")
    compute.add_argument("--limit", type=int)
    compute.add_argument("--plots-dir", type=Path)
    compute.add_argument("--plot-limit", type=int, default=12)
    compute.set_defaults(func=cmd_compute)

    similarity = sub.add_parser("similarity", help="Build bottleneck-distance DOT graph")
    similarity.add_argument("--input", type=Path, required=True)
    similarity.add_argument("--out", type=Path, required=True)
    similarity.add_argument("--dimension", type=int, default=1)
    similarity.add_argument("--threshold", type=float, default=0.5)
    similarity.set_defaults(func=cmd_similarity)
    return parser


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
