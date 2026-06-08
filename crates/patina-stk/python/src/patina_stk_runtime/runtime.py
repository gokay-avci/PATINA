from __future__ import annotations

import platform

import numpy
import rdkit
import scipy
from rdkit import Chem
from rdkit.Chem import AllChem

from .contracts import (
    CanonicalizeSmilesRequest,
    CanonicalizeSmilesResponse,
    DetectFunctionalGroupsRequest,
    DetectFunctionalGroupsResponse,
    EmbedConformerRequest,
    EmbedConformerResponse,
    FunctionalGroupMatch,
    ToolkitStatus,
)


def toolkit_status() -> ToolkitStatus:
    return ToolkitStatus(
        runtime="patina-stk-python",
        python=platform.python_version(),
        rdkit=rdkit.__version__,
        numpy=numpy.__version__,
        scipy=scipy.__version__,
    )


def canonicalize_smiles(request: CanonicalizeSmilesRequest) -> CanonicalizeSmilesResponse:
    molecule = Chem.MolFromSmiles(request.smiles)
    if molecule is None:
        raise ValueError(f"failed to parse smiles: {request.smiles}")
    return CanonicalizeSmilesResponse(canonical_smiles=Chem.MolToSmiles(molecule, canonical=True))


def detect_functional_groups(
    request: DetectFunctionalGroupsRequest,
) -> DetectFunctionalGroupsResponse:
    molecule = Chem.MolFromSmiles(request.smiles)
    if molecule is None:
        raise ValueError(f"failed to parse smiles: {request.smiles}")

    matches: list[FunctionalGroupMatch] = []
    for pattern in request.patterns:
        query = Chem.MolFromSmarts(pattern.smarts)
        if query is None:
            raise ValueError(f"failed to parse SMARTS for pattern `{pattern.kind}`")
        for atom_ids in molecule.GetSubstructMatches(query):
            mapped_atom_ids = tuple(int(atom_id) for atom_id in atom_ids)
            matches.append(
                FunctionalGroupMatch(
                    kind=pattern.kind,
                    bond_intent=pattern.bond_intent,
                    atom_ids=mapped_atom_ids,
                    bonder_atom_ids=_project_role_indices(mapped_atom_ids, pattern.bonder_indices),
                    deleter_atom_ids=_project_role_indices(mapped_atom_ids, pattern.deleter_indices),
                    placer_atom_ids=_project_role_indices(mapped_atom_ids, pattern.placer_indices),
                )
            )
    return DetectFunctionalGroupsResponse(matches=tuple(matches))


def embed_conformer(request: EmbedConformerRequest) -> EmbedConformerResponse:
    molecule = Chem.MolFromSmiles(request.smiles)
    if molecule is None:
        raise ValueError(f"failed to parse smiles: {request.smiles}")

    canonical_smiles = Chem.MolToSmiles(molecule, canonical=True)
    molecule = Chem.AddHs(molecule)

    params = AllChem.ETKDGv3()
    params.randomSeed = request.seed
    status = AllChem.EmbedMolecule(molecule, params)
    if status != 0:
        raise ValueError(f"rdkit conformer embedding failed with code {status}")

    optimize_status = AllChem.UFFOptimizeMolecule(molecule)
    if optimize_status == -1:
        raise ValueError("uff optimization could not be initialized for the embedded conformer")

    molecule = Chem.RemoveHs(molecule)
    conformer = molecule.GetConformer()
    atom_symbols: list[str] = []
    atom_positions: list[tuple[float, float, float]] = []
    for atom in molecule.GetAtoms():
        atom_symbols.append(atom.GetSymbol())
        position = conformer.GetAtomPosition(atom.GetIdx())
        atom_positions.append((float(position.x), float(position.y), float(position.z)))

    return EmbedConformerResponse(
        canonical_smiles=canonical_smiles,
        atom_symbols=tuple(atom_symbols),
        atom_positions=tuple(atom_positions),
    )


def _project_role_indices(atom_ids: tuple[int, ...], indices: tuple[int, ...]) -> tuple[int, ...]:
    projected: list[int] = []
    for index in indices:
        if index >= len(atom_ids):
            raise ValueError("functional-group role index exceeds SMARTS match width")
        projected.append(atom_ids[index])
    return tuple(projected)
