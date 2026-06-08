from __future__ import annotations

import json
import math
from pathlib import Path
from typing import Any


REQUEST_SCHEMA_VERSION = "patina.emulate.feature_projection_request.v1"
RESPONSE_SCHEMA_VERSION = "patina.emulate.feature_projection_response.v1"
SUPPORTED_PROVIDERS = {"dscribe", "featomic"}
VERSION = "v1"


def _require_mapping(raw: Any, field_name: str) -> dict[str, Any]:
    if not isinstance(raw, dict):
        raise ValueError(f"{field_name} must be a mapping")
    return raw


def _cartesian_positions(structure: dict[str, Any]) -> list[list[float]]:
    coords = structure.get("fractional_coords")
    if not isinstance(coords, list) or not coords:
        raise ValueError("structure.fractional_coords must be a non-empty list")

    lattice = structure.get("lattice")
    if lattice is None:
        return [[float(value) for value in coord] for coord in coords]

    return [
        [
            coord[0] * lattice[0][0] + coord[1] * lattice[1][0] + coord[2] * lattice[2][0],
            coord[0] * lattice[0][1] + coord[1] * lattice[1][1] + coord[2] * lattice[2][1],
            coord[0] * lattice[0][2] + coord[1] * lattice[1][2] + coord[2] * lattice[2][2],
        ]
        for coord in coords
    ]


def _atoms_from_structure(structure: dict[str, Any]):
    from ase import Atoms

    species = structure.get("species")
    if not isinstance(species, list) or not species:
        raise ValueError("structure.species must be a non-empty list")
    positions = _cartesian_positions(structure)
    if len(species) != len(positions):
        raise ValueError("structure species/coordinate lengths do not match")

    lattice = structure.get("lattice")
    pbc = structure.get("periodic_axes") or [False, False, False]
    if lattice is None:
        return Atoms(symbols=species, positions=positions, pbc=False)
    return Atoms(symbols=species, positions=positions, cell=lattice, pbc=pbc)


def _basis_species(structures: list[dict[str, Any]]) -> list[str]:
    species = sorted({str(item) for structure in structures for item in structure["species"]})
    if not species:
        raise ValueError("SOAP projection requires at least one species")
    return species


def _normalize_rows(values, normalization: str):
    import numpy as np

    rows = np.asarray(values, dtype=float)
    if rows.ndim == 1:
        rows = rows.reshape(1, -1)
    if normalization == "none":
        return rows
    if normalization != "l2":
        raise ValueError(f"unsupported SOAP normalization: {normalization}")
    norms = np.linalg.norm(rows, axis=1, keepdims=True)
    norms[norms <= 1e-15] = 1.0
    return rows / norms


def _compute_dscribe(structures: list[dict[str, Any]], config: dict[str, Any], species: list[str]):
    import numpy as np
    from dscribe.descriptors import SOAP

    atoms = [_atoms_from_structure(structure) for structure in structures]
    periodic = any(any(bool(axis) for axis in structure.get("periodic_axes", [])) for structure in structures)
    descriptor = SOAP(
        species=species,
        periodic=periodic,
        r_cut=float(config["cutoff_radius"]),
        n_max=int(config["n_max"]),
        l_max=int(config["l_max"]),
        sigma=float(config["gaussian_width"]),
        average="inner",
        sparse=False,
    )
    values = np.asarray(descriptor.create(atoms), dtype=float)
    if values.ndim == 1:
        values = values.reshape(1, -1)
    return values


def _compute_featomic(structures: list[dict[str, Any]], config: dict[str, Any], species: list[str]):
    import numpy as np
    from featomic import SoapPowerSpectrum

    atoms = [_atoms_from_structure(structure) for structure in structures]
    calculator = SoapPowerSpectrum(
        cutoff={
            "radius": float(config["cutoff_radius"]),
            "smoothing": {"type": "ShiftedCosine", "width": float(config["smoothing_width"])},
        },
        density={"type": "Gaussian", "width": float(config["gaussian_width"])},
        basis={
            "type": "TensorProduct",
            "max_angular": int(config["l_max"]),
            "radial": {"type": "Gto", "max_radial": int(config["n_max"])},
        },
    )
    descriptor = calculator.compute(atoms)
    descriptor = descriptor.keys_to_samples("center_type")
    descriptor = descriptor.keys_to_properties(["neighbor_1_type", "neighbor_2_type"])
    block = descriptor.block()
    values = np.asarray(block.values, dtype=float)
    samples = block.samples
    system_column = list(samples.names).index("system")
    system_values = np.asarray(samples.values)[:, system_column]

    feature_count = values.shape[1]
    rows = []
    for system_index in range(len(atoms)):
        mask = system_values == system_index
        if mask.any():
            rows.append(values[mask].mean(axis=0))
        else:
            rows.append(np.zeros(feature_count, dtype=float))
    return np.asarray(rows, dtype=float)


def project_features(raw: dict[str, Any]) -> dict[str, Any]:
    if raw.get("schema_version") != REQUEST_SCHEMA_VERSION:
        raise ValueError(f"unsupported feature projection schema: {raw.get('schema_version')}")

    provider = str(raw.get("provider", "")).strip().lower()
    if provider not in SUPPORTED_PROVIDERS:
        raise ValueError(f"unsupported SOAP provider: {provider}")

    config = _require_mapping(raw.get("config"), "config")
    structures_raw = raw.get("structures")
    if not isinstance(structures_raw, list) or not structures_raw:
        raise ValueError("structures must be a non-empty list")
    structures = [_require_mapping(structure, "structures[]") for structure in structures_raw]
    species = _basis_species(structures)

    if provider == "dscribe":
        values = _compute_dscribe(structures, config, species)
        family = "dscribe_soap_global"
    else:
        values = _compute_featomic(structures, config, species)
        family = "featomic_soap_global"

    values = _normalize_rows(values, str(config.get("normalization", "l2")))
    if values.shape[0] != len(structures):
        raise RuntimeError("SOAP descriptor row count does not match input structures")
    if not math.isfinite(float(values.sum())):
        raise RuntimeError("SOAP descriptor output contains non-finite values")

    feature_count = int(values.shape[1])
    prefix = "dscribe_soap" if provider == "dscribe" else "featomic_soap"
    return {
        "schema_version": RESPONSE_SCHEMA_VERSION,
        "family": family,
        "version": VERSION,
        "feature_names": [f"{prefix}_{index:05d}" for index in range(feature_count)],
        "provenance_label": (
            f"patina_emulate.soap.{provider}.{VERSION}:"
            f"cutoff={float(config['cutoff_radius']):.3f}:"
            f"n_max={int(config['n_max'])}:l_max={int(config['l_max'])}:"
            f"species={','.join(species)}"
        ),
        "vectors": [{"features": row.tolist()} for row in values],
    }


def load_feature_request(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text())


def write_feature_response(path: Path, response: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(response, indent=2, sort_keys=True) + "\n")
