#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import math
from dataclasses import dataclass
from pathlib import Path

import matplotlib.pyplot as plt


@dataclass
class CandidateRow:
    family: str
    feature_count: int
    candidate_id: str
    label: str
    rank: int
    predicted_mean: float
    predicted_variance: float
    uncertainty_score: float
    exact_energy: float | None


def _normalize_candidate_label(raw: str) -> str:
    parts = raw.split("_", 1)
    if len(parts) == 2 and parts[0].isdigit():
        return parts[1]
    return raw


def _parse_mapping_arg(raw: str) -> tuple[str, Path]:
    if "=" not in raw:
        raise argparse.ArgumentTypeError(f"expected <name>=<path>, got: {raw}")
    name, path = raw.split("=", 1)
    return name.strip(), Path(path).expanduser().resolve()


def _load_json(path: Path) -> dict:
    return json.loads(path.read_text())


def _safe_mean(values: list[float]) -> float | None:
    return sum(values) / len(values) if values else None


def _safe_rmse(errors: list[float]) -> float | None:
    if not errors:
        return None
    return math.sqrt(sum(value * value for value in errors) / len(errors))


def _rankdata(values: list[float]) -> list[float]:
    indexed = sorted(enumerate(values), key=lambda item: item[1])
    ranks = [0.0] * len(values)
    i = 0
    while i < len(indexed):
        j = i + 1
        while j < len(indexed) and indexed[j][1] == indexed[i][1]:
            j += 1
        avg_rank = (i + 1 + j) / 2.0
        for k in range(i, j):
            ranks[indexed[k][0]] = avg_rank
        i = j
    return ranks


def _spearman(xs: list[float], ys: list[float]) -> float | None:
    if len(xs) < 2 or len(xs) != len(ys):
        return None
    xr = _rankdata(xs)
    yr = _rankdata(ys)
    x_mean = sum(xr) / len(xr)
    y_mean = sum(yr) / len(yr)
    numerator = sum((x - x_mean) * (y - y_mean) for x, y in zip(xr, yr))
    x_denom = math.sqrt(sum((x - x_mean) ** 2 for x in xr))
    y_denom = math.sqrt(sum((y - y_mean) ** 2 for y in yr))
    if x_denom <= 1e-12 or y_denom <= 1e-12:
        return None
    return numerator / (x_denom * y_denom)


def load_exact_energy_map(run_dir: Path) -> dict[str, float]:
    exact_by_label: dict[str, float] = {}
    for path in sorted((run_dir / "raw").glob("generation_*_state.json")):
        state = _load_json(path)
        for member in state.get("population", []):
            source = member.get("source", {})
            evaluation = member.get("evaluation", {})
            label = (
                evaluation.get("label")
                or source.get("label")
                or member.get("source_candidate_label")
                or member.get("evaluation_label")
            )
            energy = evaluation.get("energy")
            if isinstance(label, str) and isinstance(energy, (int, float)):
                exact_by_label.setdefault(label, float(energy))
    return exact_by_label


def load_generation_progress(run_dir: Path) -> list[dict]:
    rows = []
    for path in sorted((run_dir / "raw").glob("generation_*_summary.json")):
        payload = _load_json(path)
        if "best_energy" in payload and "generation" in payload:
            rows.append(payload)
    return rows


def load_candidate_rows(name: str, artifact_dir: Path, exact_by_label: dict[str, float]) -> list[CandidateRow]:
    request = _load_json(artifact_dir / "request.json")
    response = _load_json(artifact_dir / "response.json")
    candidate_labels = {
        row["candidate_id"]: _normalize_candidate_label(
            row.get("metadata", {}).get("label", row["candidate_id"])
        )
        for row in request["candidate_rows"]
    }
    family = request["context"]["feature_representation"]["family"]
    feature_count = len(request["feature_names"])
    rows = []
    for prediction in response["predictions"]:
        label = candidate_labels[prediction["candidate_id"]]
        rows.append(
            CandidateRow(
                family=name if name else family,
                feature_count=feature_count,
                candidate_id=prediction["candidate_id"],
                label=label,
                rank=int(prediction["rank"]),
                predicted_mean=float(prediction["means"][0]),
                predicted_variance=float(prediction["variances"][0]),
                uncertainty_score=float(prediction["uncertainty_score"]),
                exact_energy=exact_by_label.get(label),
            )
        )
    rows.sort(key=lambda row: row.rank)
    return rows


def write_summary_csv(path: Path, rows: list[CandidateRow]) -> None:
    header = (
        "family,feature_count,candidate_id,label,rank,predicted_mean,predicted_variance,"
        "uncertainty_score,exact_energy,abs_error\n"
    )
    lines = [header]
    for row in rows:
        abs_error = (
            abs(row.predicted_mean - row.exact_energy) if row.exact_energy is not None else ""
        )
        lines.append(
            ",".join(
                [
                    row.family,
                    str(row.feature_count),
                    row.candidate_id,
                    row.label,
                    str(row.rank),
                    f"{row.predicted_mean:.8f}",
                    f"{row.predicted_variance:.8f}",
                    f"{row.uncertainty_score:.8f}",
                    "" if row.exact_energy is None else f"{row.exact_energy:.8f}",
                    "" if abs_error == "" else f"{abs_error:.8f}",
                ]
            )
            + "\n"
        )
    path.write_text("".join(lines))


def write_family_summary_json(path: Path, rows: list[CandidateRow]) -> None:
    grouped: dict[str, list[CandidateRow]] = {}
    for row in rows:
        grouped.setdefault(row.family, []).append(row)

    payload = {}
    for family, family_rows in grouped.items():
        exact_rows = [row for row in family_rows if row.exact_energy is not None]
        errors = [row.predicted_mean - row.exact_energy for row in exact_rows]
        abs_errors = [abs(value) for value in errors]
        predicted = [row.predicted_mean for row in exact_rows]
        exact = [row.exact_energy for row in exact_rows if row.exact_energy is not None]
        top = family_rows[0]
        payload[family] = {
            "feature_count": family_rows[0].feature_count,
            "candidate_count": len(family_rows),
            "top_ranked_label": top.label,
            "top_ranked_predicted_mean": top.predicted_mean,
            "top_ranked_exact_energy": top.exact_energy,
            "mean_uncertainty_score": _safe_mean([row.uncertainty_score for row in family_rows]),
            "mean_predicted_variance": _safe_mean([row.predicted_variance for row in family_rows]),
            "mae": _safe_mean(abs_errors),
            "rmse": _safe_rmse(errors),
            "spearman_predicted_vs_exact": _spearman(predicted, exact),
        }
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


def plot_emulate_comparison(path: Path, rows: list[CandidateRow]) -> None:
    grouped: dict[str, list[CandidateRow]] = {}
    for row in rows:
        grouped.setdefault(row.family, []).append(row)

    families = list(grouped)
    mae_values = []
    mean_uncertainties = []
    feature_counts = []
    for family in families:
        family_rows = grouped[family]
        exact_rows = [row for row in family_rows if row.exact_energy is not None]
        mae_values.append(
            _safe_mean([abs(row.predicted_mean - row.exact_energy) for row in exact_rows]) or 0.0
        )
        mean_uncertainties.append(
            _safe_mean([row.uncertainty_score for row in family_rows]) or 0.0
        )
        feature_counts.append(family_rows[0].feature_count)

    fig, axes = plt.subplots(2, 2, figsize=(14, 10))
    ax_mae, ax_uncertainty, ax_scatter, ax_rank = axes.flatten()

    ax_mae.bar(families, mae_values, color=["#1f77b4", "#ff7f0e", "#2ca02c", "#d62728"])
    ax_mae.set_title("Pending-candidate MAE")
    ax_mae.set_ylabel("Abs error (eV)")
    ax_mae.tick_params(axis="x", rotation=20)

    ax_uncertainty.bar(
        families,
        mean_uncertainties,
        color=["#4c78a8", "#f58518", "#54a24b", "#e45756"],
    )
    for idx, feature_count in enumerate(feature_counts):
        ax_uncertainty.text(idx, mean_uncertainties[idx], f"{feature_count} f", ha="center", va="bottom")
    ax_uncertainty.set_title("Mean uncertainty score")
    ax_uncertainty.set_ylabel("Score")
    ax_uncertainty.tick_params(axis="x", rotation=20)

    for family in families:
        family_rows = [row for row in grouped[family] if row.exact_energy is not None]
        ax_scatter.scatter(
            [row.exact_energy for row in family_rows],
            [row.predicted_mean for row in family_rows],
            label=family,
            s=40,
            alpha=0.8,
        )
    exact_values = [row.exact_energy for row in rows if row.exact_energy is not None]
    predicted_values = [row.predicted_mean for row in rows if row.exact_energy is not None]
    lo = min(exact_values + predicted_values)
    hi = max(exact_values + predicted_values)
    ax_scatter.plot([lo, hi], [lo, hi], linestyle="--", color="black", linewidth=1)
    ax_scatter.set_title("Predicted vs exact pending energies")
    ax_scatter.set_xlabel("Exact energy (eV)")
    ax_scatter.set_ylabel("Predicted energy (eV)")
    ax_scatter.legend(frameon=False, fontsize=9)

    candidate_labels = sorted({row.label for row in rows})
    rank_matrix = []
    for family in families:
        rank_by_label = {row.label: row.rank for row in grouped[family]}
        rank_matrix.append([rank_by_label[label] for label in candidate_labels])
    image = ax_rank.imshow(rank_matrix, aspect="auto", cmap="viridis_r")
    ax_rank.set_title("Advisory rank by descriptor family")
    ax_rank.set_yticks(range(len(families)), labels=families)
    ax_rank.set_xticks(range(len(candidate_labels)), labels=candidate_labels, rotation=75, ha="right")
    fig.colorbar(image, ax=ax_rank, label="Rank")

    fig.tight_layout()
    fig.savefig(path, dpi=200)
    plt.close(fig)


def plot_progress(path: Path, progress_runs: list[tuple[str, Path]]) -> None:
    fig, axes = plt.subplots(len(progress_runs), 2, figsize=(14, 4 * len(progress_runs)), squeeze=False)
    for row_index, (label, run_dir) in enumerate(progress_runs):
        progress = load_generation_progress(run_dir)
        generations = [item["generation"] for item in progress]
        best = [item["best_energy"] for item in progress]
        mean = [item["mean_energy"] for item in progress]
        worst = [item["worst_energy"] for item in progress]
        failures = [item["failure_count"] for item in progress]
        duplicates = [item["duplicate_count"] for item in progress]
        repopulated = [item["repopulated_count"] for item in progress]

        ax_energy = axes[row_index][0]
        ax_counts = axes[row_index][1]
        ax_energy.plot(generations, best, marker="o", label="best")
        ax_energy.plot(generations, mean, marker="o", label="mean")
        ax_energy.plot(generations, worst, marker="o", label="worst")
        ax_energy.set_title(f"{label}: generation energy progression")
        ax_energy.set_xlabel("Generation")
        ax_energy.set_ylabel("Energy")
        ax_energy.legend(frameon=False)

        ax_counts.plot(generations, failures, marker="o", label="failures")
        ax_counts.plot(generations, duplicates, marker="o", label="duplicates")
        ax_counts.plot(generations, repopulated, marker="o", label="repopulated")
        ax_counts.set_title(f"{label}: generation control counts")
        ax_counts.set_xlabel("Generation")
        ax_counts.set_ylabel("Count")
        ax_counts.legend(frameon=False)

    fig.tight_layout()
    fig.savefig(path, dpi=200)
    plt.close(fig)


def main() -> None:
    parser = argparse.ArgumentParser(description="Render Ti3N4 emulate comparison plots.")
    parser.add_argument("--run-dir", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument(
        "--comparison",
        action="append",
        type=_parse_mapping_arg,
        required=True,
        help="Descriptor family name and surrogate artifact dir, as <name>=<path>",
    )
    parser.add_argument(
        "--progress-run",
        action="append",
        type=_parse_mapping_arg,
        default=[],
        help="Labeled run dir for progress plots, as <name>=<path>",
    )
    args = parser.parse_args()

    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    exact_by_label = load_exact_energy_map(args.run_dir.resolve())
    candidate_rows = []
    for name, artifact_dir in args.comparison:
        candidate_rows.extend(load_candidate_rows(name, artifact_dir, exact_by_label))

    write_summary_csv(output_dir / "descriptor_candidate_comparison.csv", candidate_rows)
    write_family_summary_json(output_dir / "descriptor_family_summary.json", candidate_rows)
    plot_emulate_comparison(output_dir / "descriptor_family_comparison.png", candidate_rows)

    if args.progress_run:
        plot_progress(output_dir / "run_progress.png", args.progress_run)


if __name__ == "__main__":
    main()
