from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any


@dataclass(slots=True, frozen=True)
class ToolkitStatus:
    runtime: str
    python: str
    rdkit: str
    numpy: str
    scipy: str

    def to_json(self) -> str:
        return json.dumps(asdict(self), sort_keys=True)


def write_status(path: Path, status: ToolkitStatus) -> None:
    path.write_text(status.to_json() + "\n", encoding="utf-8")


@dataclass(slots=True, frozen=True)
class CanonicalizeSmilesRequest:
    smiles: str

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "CanonicalizeSmilesRequest":
        smiles = raw.get("smiles")
        if not isinstance(smiles, str) or not smiles.strip():
            raise ValueError("smiles must be a non-empty string")
        return cls(smiles=smiles)


@dataclass(slots=True, frozen=True)
class CanonicalizeSmilesResponse:
    canonical_smiles: str

    def to_json(self) -> str:
        return json.dumps(asdict(self), sort_keys=True)


@dataclass(slots=True, frozen=True)
class FunctionalGroupPattern:
    kind: str
    smarts: str
    bond_intent: str
    bonder_indices: tuple[int, ...]
    deleter_indices: tuple[int, ...]
    placer_indices: tuple[int, ...]

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "FunctionalGroupPattern":
        kind = raw.get("kind")
        smarts = raw.get("smarts")
        if not isinstance(kind, str) or not kind.strip():
            raise ValueError("pattern.kind must be a non-empty string")
        if not isinstance(smarts, str) or not smarts.strip():
            raise ValueError("pattern.smarts must be a non-empty string")
        return cls(
            kind=kind,
            smarts=smarts,
            bond_intent=_read_bond_intent(raw.get("bond_intent")),
            bonder_indices=_read_index_tuple(raw.get("bonder_indices")),
            deleter_indices=_read_index_tuple(raw.get("deleter_indices")),
            placer_indices=_read_index_tuple(raw.get("placer_indices")),
        )


@dataclass(slots=True, frozen=True)
class FunctionalGroupMatch:
    kind: str
    bond_intent: str
    atom_ids: tuple[int, ...]
    bonder_atom_ids: tuple[int, ...]
    deleter_atom_ids: tuple[int, ...]
    placer_atom_ids: tuple[int, ...]


@dataclass(slots=True, frozen=True)
class DetectFunctionalGroupsRequest:
    smiles: str
    patterns: tuple[FunctionalGroupPattern, ...]

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "DetectFunctionalGroupsRequest":
        smiles = raw.get("smiles")
        if not isinstance(smiles, str) or not smiles.strip():
            raise ValueError("smiles must be a non-empty string")
        patterns_raw = raw.get("patterns")
        if not isinstance(patterns_raw, list) or not patterns_raw:
            raise ValueError("patterns must be a non-empty list")
        return cls(
            smiles=smiles,
            patterns=tuple(FunctionalGroupPattern.from_mapping(item) for item in patterns_raw),
        )


@dataclass(slots=True, frozen=True)
class DetectFunctionalGroupsResponse:
    matches: tuple[FunctionalGroupMatch, ...]

    def to_json(self) -> str:
        return json.dumps(asdict(self), sort_keys=True)


@dataclass(slots=True, frozen=True)
class EmbedConformerRequest:
    smiles: str
    seed: int

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "EmbedConformerRequest":
        smiles = raw.get("smiles")
        seed = raw.get("seed")
        if not isinstance(smiles, str) or not smiles.strip():
            raise ValueError("smiles must be a non-empty string")
        if not isinstance(seed, int) or seed < 0:
            raise ValueError("seed must be a non-negative integer")
        return cls(smiles=smiles, seed=seed)


@dataclass(slots=True, frozen=True)
class EmbedConformerResponse:
    canonical_smiles: str
    atom_symbols: tuple[str, ...]
    atom_positions: tuple[tuple[float, float, float], ...]

    def to_json(self) -> str:
        return json.dumps(asdict(self), sort_keys=True)


def load_json_input() -> dict[str, Any]:
    return json.load(__import__("sys").stdin)


def _read_index_tuple(raw: Any) -> tuple[int, ...]:
    if raw is None:
        return ()
    if not isinstance(raw, list):
        raise ValueError("role index fields must be lists when present")
    values: list[int] = []
    for item in raw:
        if not isinstance(item, int) or item < 0:
            raise ValueError("role index fields must contain non-negative integers")
        values.append(item)
    return tuple(values)


def _read_bond_intent(raw: Any) -> str:
    if raw is None:
        return "covalent"
    if raw not in {"covalent", "coordination"}:
        raise ValueError("bond_intent must be `covalent` or `coordination`")
    return raw
