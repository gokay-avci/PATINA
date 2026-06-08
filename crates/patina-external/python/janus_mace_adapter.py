#!/usr/bin/env python3
from __future__ import annotations

import argparse
import contextlib
import json
import math
import os
import sys
from pathlib import Path


def _load_atoms(xyz_path: Path):
    try:
        from ase.io import read as ase_read
    except ImportError as exc:
        raise RuntimeError(
            "ASE is required for the Janus/MACE adapter. Install it in the selected environment."
        ) from exc
    return ase_read(str(xyz_path))


def _single_point(atoms, *, arch: str, model: str, device: str, dtype: str):
    calc = _load_calculator(arch=arch, model=model, device=device, dtype=dtype)
    atoms.calc = calc
    energy = float(atoms.get_potential_energy())
    forces = atoms.get_forces()
    return atoms, energy, forces, True, 0


def _load_calculator(*, arch: str, model: str, device: str, dtype: str):
    try:
        from janus_core.helpers.mlip_calculators import choose_calculator  # type: ignore
    except ImportError as exc:
        raise RuntimeError(
            "janus-core with MACE support is required. Install `janus-core[mace]` in the selected environment."
        ) from exc

    result = choose_calculator(
        arch=arch,
        device=device,
        model=model,
        default_dtype=dtype,
    )
    return result[0] if isinstance(result, tuple) else result


def _single_point_with_calc(atoms, calc):
    atoms.calc = calc
    energy = float(atoms.get_potential_energy())
    forces = atoms.get_forces()
    return atoms, energy, forces, True, 0


def _build_optimizer(name: str):
    name = name.lower()
    if name == "lbfgs":
        from ase.optimize import LBFGS

        return LBFGS, {}
    if name == "fire":
        from ase.optimize import FIRE

        return FIRE, {}
    if name == "fire2":
        try:
            from ase.optimize import FIRE2

            return FIRE2, {"use_abc": False}
        except ImportError:
            raise RuntimeError(
                "ASE FIRE2 optimizer is not available in the selected environment."
            )
    if name == "abc-fire":
        try:
            from ase.optimize import FIRE2

            return FIRE2, {"use_abc": True}
        except ImportError:
            raise RuntimeError(
                "ASE FIRE2/ABC-FIRE optimizer is not available in the selected environment."
            )
    raise RuntimeError(f"unsupported optimizer: {name}")


def _local_opt(
    xyz_path: Path,
    *,
    arch: str,
    model: str,
    device: str,
    dtype: str,
    optimizer: str,
    fmax: float,
    steps: int,
):
    try:
        from ase.io import read as ase_read
    except ImportError as exc:
        raise RuntimeError(
            "janus-core geometry optimization support is required. Install `janus-core[mace]` in the selected environment."
        ) from exc

    atoms = ase_read(str(xyz_path))
    calc = _load_calculator(arch=arch, model=model, device=device, dtype=dtype)
    atoms.calc = calc
    optimizer_cls, optimizer_kwargs = _build_optimizer(optimizer)
    optimizer_obj = optimizer_cls(atoms, logfile=None, **optimizer_kwargs)
    converged = bool(optimizer_obj.run(fmax=fmax, steps=steps))
    energy = float(atoms.get_potential_energy())
    forces = atoms.get_forces()
    return atoms, energy, forces, converged, int(optimizer_obj.get_number_of_steps())


def _local_opt_with_calc(xyz_path: Path, *, calc, optimizer: str, fmax: float, steps: int):
    try:
        from ase.io import read as ase_read
    except ImportError as exc:
        raise RuntimeError(
            "ASE geometry optimization support is required for the Janus/MACE adapter."
        ) from exc

    atoms = ase_read(str(xyz_path))
    atoms.calc = calc
    optimizer_cls, optimizer_kwargs = _build_optimizer(optimizer)
    optimizer_obj = optimizer_cls(atoms, logfile=None, **optimizer_kwargs)
    converged = bool(optimizer_obj.run(fmax=fmax, steps=steps))
    energy = float(atoms.get_potential_energy())
    forces = atoms.get_forces()
    return atoms, energy, forces, converged, int(optimizer_obj.get_number_of_steps())


def _max_force(forces) -> float:
    max_force = 0.0
    for fx, fy, fz in forces:
        magnitude = math.sqrt(float(fx) ** 2 + float(fy) ** 2 + float(fz) ** 2)
        if magnitude > max_force:
            max_force = magnitude
    return float(max_force)


def _response(
    atoms,
    energy: float,
    forces,
    *,
    converged: bool,
    n_steps: int,
    failure_reason: str | None = None,
) -> dict[str, object]:
    symbols = [str(symbol) for symbol in atoms.get_chemical_symbols()]
    positions = atoms.get_positions()
    has_periodicity = bool(atoms.pbc is not None and atoms.pbc.any())
    lattice = atoms.cell.array.tolist() if has_periodicity else None
    periodic_axes = [bool(value) for value in atoms.pbc] if has_periodicity else None
    force_rows = [[float(x), float(y), float(z)] for x, y, z in forces]
    coord_rows = [[float(x), float(y), float(z)] for x, y, z in positions]
    return {
        "energy": float(energy),
        "forces": force_rows,
        "species": symbols,
        "coords": coord_rows,
        "lattice": lattice,
        "periodic_axes": periodic_axes,
        "converged": bool(converged),
        "max_force": _max_force(force_rows),
        "n_steps": int(n_steps),
        "failure_reason": failure_reason,
    }


def _ensure_finite(energy: float, forces) -> None:
    if not math.isfinite(float(energy)):
        raise RuntimeError("Janus/MACE returned a non-finite energy.")
    for row in forces:
        for value in row:
            if not math.isfinite(float(value)):
                raise RuntimeError("Janus/MACE returned non-finite forces.")


def _ensure_finite_coords(atoms) -> None:
    for row in atoms.get_positions():
        for value in row:
            if not math.isfinite(float(value)):
                raise RuntimeError("Janus/MACE returned non-finite coordinates.")


def _write_xyz(path: Path, atoms, energy: float) -> None:
    symbols = [str(symbol) for symbol in atoms.get_chemical_symbols()]
    positions = atoms.get_positions()
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        handle.write(f"{len(symbols)}\n")
        handle.write(f"Janus energy {float(energy):.12f}\n")
        for symbol, (x, y, z) in zip(symbols, positions):
            handle.write(f"{symbol} {float(x):.10f} {float(y):.10f} {float(z):.10f}\n")


def _write_summary(path: Path, result: dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as handle:
        handle.write(f"energy={float(result['energy']):.12f}\n")
        handle.write(f"converged={'true' if result['converged'] else 'false'}\n")
        handle.write(f"max_force={float(result['max_force']):.12f}\n")
        handle.write(f"n_steps={int(result['n_steps'])}\n")
        failure_reason = result.get("failure_reason")
        if failure_reason is None:
            handle.write("failure_reason=\n")
        else:
            handle.write(f"failure_reason={str(failure_reason)}\n")


def _emit_json_line(payload: dict[str, object]) -> None:
    sys.stdout.write(json.dumps(payload, allow_nan=False) + "\n")
    sys.stdout.flush()


@contextlib.contextmanager
def _protocol_stdout_guard():
    """Reserve stdout for protocol JSON and redirect third-party prints to stderr."""
    with contextlib.redirect_stdout(sys.stderr):
        yield


def _serve(args) -> int:
    with _protocol_stdout_guard():
        calc = _load_calculator(
            arch=args.arch,
            model=args.model,
            device=args.device,
            dtype=args.dtype,
        )
    _emit_json_line({"status": "ready"})

    for raw_line in sys.stdin:
        raw_line = raw_line.strip()
        if not raw_line:
            continue
        try:
            request = json.loads(raw_line)
            command = request.get("command", "evaluate")
            request_id = request.get("request_id")

            if command == "shutdown":
                _emit_json_line({"status": "ok", "request_id": request_id})
                return 0

            if command != "evaluate":
                raise RuntimeError(f"unsupported serve command: {command}")

            input_path = Path(request["input"])
            with _protocol_stdout_guard():
                if args.mode == "local_opt":
                    atoms, energy, forces, converged, n_steps = _local_opt_with_calc(
                        input_path,
                        calc=calc,
                        optimizer=args.optimizer,
                        fmax=args.fmax,
                        steps=args.steps,
                    )
                else:
                    atoms = _load_atoms(input_path)
                    atoms, energy, forces, converged, n_steps = _single_point_with_calc(
                        atoms, calc
                    )

            _ensure_finite(energy, forces)
            _ensure_finite_coords(atoms)
            failure_reason = None if converged else "optimizer_not_converged"
            result = _response(
                atoms,
                energy,
                forces,
                converged=bool(converged),
                n_steps=n_steps,
                failure_reason=failure_reason,
            )
            _emit_json_line({"status": "ok", "request_id": request_id, "result": result})
        except Exception as exc:  # pragma: no cover - integration surface
            _emit_json_line(
                {
                    "status": "error",
                    "request_id": request.get("request_id") if "request" in locals() else None,
                    "error": str(exc),
                }
            )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--output-xyz", type=Path)
    parser.add_argument("--output-energy", type=Path)
    parser.add_argument("--output-summary", type=Path)
    parser.add_argument("--serve", action="store_true")
    parser.add_argument("--mode", choices=["single_point", "local_opt"], default="single_point")
    parser.add_argument("--arch", default="mace_mp")
    parser.add_argument("--model", default="small")
    parser.add_argument("--device", default="cpu")
    parser.add_argument("--dtype", default="float64")
    parser.add_argument(
        "--optimizer",
        choices=["lbfgs", "fire", "fire2", "abc-fire"],
        default="abc-fire",
    )
    parser.add_argument("--fmax", type=float, default=0.1)
    parser.add_argument("--steps", type=int, default=100)
    args = parser.parse_args()
    if args.serve:
        cache_dir = Path.cwd() / ".janus-cache"
    else:
        if args.input is None or args.output is None:
            parser.error("--input and --output are required unless --serve is used")
        cache_dir = args.output.parent / ".janus-cache"
    cache_dir.mkdir(parents=True, exist_ok=True)
    os.environ.setdefault("MPLCONFIGDIR", str(cache_dir))

    try:
        if args.serve:
            return _serve(args)
        with _protocol_stdout_guard():
            if args.mode == "local_opt":
                atoms, energy, forces, converged, n_steps = _local_opt(
                    args.input,
                    arch=args.arch,
                    model=args.model,
                    device=args.device,
                    dtype=args.dtype,
                    optimizer=args.optimizer,
                    fmax=args.fmax,
                    steps=args.steps,
                )
            else:
                atoms = _load_atoms(args.input)
                atoms, energy, forces, converged, n_steps = _single_point(
                    atoms,
                    arch=args.arch,
                    model=args.model,
                    device=args.device,
                    dtype=args.dtype,
                )
        _ensure_finite(energy, forces)
        _ensure_finite_coords(atoms)
        failure_reason = None if converged else "optimizer_not_converged"
        result = _response(
            atoms,
            energy,
            forces,
            converged=bool(converged),
            n_steps=n_steps,
            failure_reason=failure_reason,
        )
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
        if args.output_xyz is not None:
            _write_xyz(args.output_xyz, atoms, energy)
        if args.output_energy is not None:
            args.output_energy.parent.mkdir(parents=True, exist_ok=True)
            args.output_energy.write_text(f"{float(energy):.12f}\n", encoding="utf-8")
        if args.output_summary is not None:
            _write_summary(args.output_summary, result)
        return 0
    except Exception as exc:  # pragma: no cover - integration surface
        print(str(exc), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())

