from __future__ import annotations

import json
import math
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


REQUEST_SCHEMA_VERSION = "patina.emulate.surrogate_request.v1"
RESPONSE_SCHEMA_VERSION = "patina.emulate.surrogate_response.v1"
SUPPORTED_WORKFLOW = "genetic_algorithm"
SUPPORTED_TASK = "score_candidates"


def _require_non_empty_string(value: Any, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{field_name} must be a non-empty string")
    return value


def _require_finite_vector(raw: Any, field_name: str) -> tuple[float, ...]:
    if not isinstance(raw, list) or not raw:
        raise ValueError(f"{field_name} must be a non-empty list")
    values = []
    for item in raw:
        if not isinstance(item, (int, float)) or not math.isfinite(item):
            raise ValueError(f"{field_name} entries must be finite numbers")
        values.append(float(item))
    return tuple(values)


def _coerce_string_map(raw: Any) -> dict[str, str]:
    if raw is None:
        return {}
    if not isinstance(raw, dict):
        raise ValueError("metadata must be a mapping when present")
    return {str(key): str(value) for key, value in raw.items()}


@dataclass(slots=True, frozen=True)
class TrainingRow:
    candidate_id: str
    features: tuple[float, ...]
    targets: tuple[float, ...]
    metadata: dict[str, str] = field(default_factory=dict)

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "TrainingRow":
        candidate_id = _require_non_empty_string(raw.get("candidate_id"), "training.candidate_id")
        features = _require_finite_vector(raw.get("features"), "training.features")
        targets = _require_finite_vector(raw.get("targets"), "training.targets")
        return cls(
            candidate_id=candidate_id,
            features=features,
            targets=targets,
            metadata=_coerce_string_map(raw.get("metadata")),
        )


@dataclass(slots=True, frozen=True)
class CandidateRow:
    candidate_id: str
    features: tuple[float, ...]
    metadata: dict[str, str] = field(default_factory=dict)

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "CandidateRow":
        candidate_id = _require_non_empty_string(raw.get("candidate_id"), "candidate.candidate_id")
        features = _require_finite_vector(raw.get("features"), "candidate.features")
        return cls(
            candidate_id=candidate_id,
            features=features,
            metadata=_coerce_string_map(raw.get("metadata")),
        )


@dataclass(slots=True, frozen=True)
class SurrogateCheckpointConfig:
    checkpoint_dir: str
    resume: bool = True
    save: bool = True

    @classmethod
    def from_mapping(cls, raw: Any) -> "SurrogateCheckpointConfig | None":
        if raw is None:
            return None
        if not isinstance(raw, dict):
            raise ValueError("surrogate.checkpoint must be a mapping when present")
        checkpoint_dir = _require_non_empty_string(
            raw.get("checkpoint_dir"), "surrogate.checkpoint.checkpoint_dir"
        )
        resume = bool(raw.get("resume", True))
        save = bool(raw.get("save", True))
        if not resume and not save:
            raise ValueError("surrogate.checkpoint must enable resume or save")
        return cls(checkpoint_dir=checkpoint_dir, resume=resume, save=save)


@dataclass(slots=True, frozen=True)
class SurrogateConfig:
    model_variant: str
    epochs: int = 100
    num_inducing: int = 24
    batch_size: int = 32
    lr: float = 0.05
    n_bootstraps: int = 1
    random_seed: int = 42
    log_level: str = "warning"
    objective: str = "minimize"
    primary_target_index: int = 0
    checkpoint: SurrogateCheckpointConfig | None = None

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "SurrogateConfig":
        if not isinstance(raw, dict):
            raise ValueError("surrogate must be a mapping")

        model_variant = _require_non_empty_string(raw.get("model_variant"), "surrogate.model_variant")
        objective = str(raw.get("objective", "minimize")).strip().lower()
        if objective not in {"minimize", "maximize"}:
            raise ValueError("surrogate.objective must be either minimize or maximize")

        config = cls(
            model_variant=model_variant,
            epochs=int(raw.get("epochs", 100)),
            num_inducing=int(raw.get("num_inducing", 24)),
            batch_size=int(raw.get("batch_size", 32)),
            lr=float(raw.get("lr", 0.05)),
            n_bootstraps=int(raw.get("n_bootstraps", 1)),
            random_seed=int(raw.get("random_seed", 42)),
            log_level=str(raw.get("log_level", "warning")),
            objective=objective,
            primary_target_index=int(raw.get("primary_target_index", 0)),
            checkpoint=SurrogateCheckpointConfig.from_mapping(raw.get("checkpoint")),
        )
        if config.epochs < 1:
            raise ValueError("surrogate.epochs must be positive")
        if config.num_inducing < 2:
            raise ValueError("surrogate.num_inducing must be at least 2")
        if config.batch_size < 1:
            raise ValueError("surrogate.batch_size must be positive")
        if config.lr <= 0.0 or not math.isfinite(config.lr):
            raise ValueError("surrogate.lr must be positive and finite")
        if config.n_bootstraps < 1:
            raise ValueError("surrogate.n_bootstraps must be positive")
        if config.primary_target_index < 0:
            raise ValueError("surrogate.primary_target_index must be non-negative")
        return config


@dataclass(slots=True, frozen=True)
class SurrogateRequest:
    schema_version: str
    campaign_id: str
    branch_id: str
    workflow: str
    task: str
    feature_names: tuple[str, ...]
    target_names: tuple[str, ...]
    training_rows: tuple[TrainingRow, ...]
    candidate_rows: tuple[CandidateRow, ...]
    surrogate: SurrogateConfig

    @classmethod
    def from_mapping(cls, raw: dict[str, Any]) -> "SurrogateRequest":
        if not isinstance(raw, dict):
            raise ValueError("request must be a mapping")

        schema_version = _require_non_empty_string(raw.get("schema_version"), "schema_version")
        if schema_version != REQUEST_SCHEMA_VERSION:
            raise ValueError(f"unsupported request schema_version: {schema_version}")

        workflow = _require_non_empty_string(raw.get("workflow"), "workflow")
        if workflow != SUPPORTED_WORKFLOW:
            raise ValueError(f"unsupported workflow: {workflow}")

        task = _require_non_empty_string(raw.get("task"), "task")
        if task != SUPPORTED_TASK:
            raise ValueError(f"unsupported task: {task}")

        feature_names_raw = raw.get("feature_names")
        if not isinstance(feature_names_raw, list) or not feature_names_raw:
            raise ValueError("feature_names must be a non-empty list")
        feature_names = tuple(_require_non_empty_string(item, "feature_names[]") for item in feature_names_raw)

        target_names_raw = raw.get("target_names")
        if not isinstance(target_names_raw, list) or not target_names_raw:
            raise ValueError("target_names must be a non-empty list")
        target_names = tuple(_require_non_empty_string(item, "target_names[]") for item in target_names_raw)

        training_raw = raw.get("training_rows")
        if not isinstance(training_raw, list) or len(training_raw) < 3:
            raise ValueError("training_rows must contain at least three observations")
        training_rows = tuple(TrainingRow.from_mapping(item) for item in training_raw)

        candidate_raw = raw.get("candidate_rows")
        if not isinstance(candidate_raw, list) or not candidate_raw:
            raise ValueError("candidate_rows must contain at least one candidate")
        candidate_rows = tuple(CandidateRow.from_mapping(item) for item in candidate_raw)

        surrogate = SurrogateConfig.from_mapping(raw.get("surrogate", {}))

        feature_count = len(feature_names)
        target_count = len(target_names)
        for row in training_rows:
            if len(row.features) != feature_count:
                raise ValueError("training row feature length does not match feature_names")
            if len(row.targets) != target_count:
                raise ValueError("training row target length does not match target_names")
        for row in candidate_rows:
            if len(row.features) != feature_count:
                raise ValueError("candidate row feature length does not match feature_names")
        if surrogate.primary_target_index >= target_count:
            raise ValueError("surrogate.primary_target_index exceeds target_names length")

        return cls(
            schema_version=schema_version,
            campaign_id=_require_non_empty_string(raw.get("campaign_id"), "campaign_id"),
            branch_id=_require_non_empty_string(raw.get("branch_id"), "branch_id"),
            workflow=workflow,
            task=task,
            feature_names=feature_names,
            target_names=target_names,
            training_rows=training_rows,
            candidate_rows=candidate_rows,
            surrogate=surrogate,
        )


@dataclass(slots=True, frozen=True)
class CandidatePrediction:
    candidate_id: str
    means: tuple[float, ...]
    variances: tuple[float, ...]
    acquisition_score: float
    uncertainty_score: float
    rank: int

    def to_mapping(self) -> dict[str, Any]:
        return {
            "candidate_id": self.candidate_id,
            "means": list(self.means),
            "variances": list(self.variances),
            "acquisition_score": self.acquisition_score,
            "uncertainty_score": self.uncertainty_score,
            "rank": self.rank,
        }


@dataclass(slots=True, frozen=True)
class SurrogateResponse:
    schema_version: str
    campaign_id: str
    branch_id: str
    workflow: str
    task: str
    feature_names: tuple[str, ...]
    target_names: tuple[str, ...]
    model_variant: str
    selected_model_name: str
    incumbent_target: float
    predictions: tuple[CandidatePrediction, ...]
    checkpoint_path: str | None = None

    def to_mapping(self) -> dict[str, Any]:
        payload = {
            "schema_version": self.schema_version,
            "campaign_id": self.campaign_id,
            "branch_id": self.branch_id,
            "workflow": self.workflow,
            "task": self.task,
            "feature_names": list(self.feature_names),
            "target_names": list(self.target_names),
            "model_variant": self.model_variant,
            "selected_model_name": self.selected_model_name,
            "incumbent_target": self.incumbent_target,
            "predictions": [prediction.to_mapping() for prediction in self.predictions],
        }
        if self.checkpoint_path is not None:
            payload["checkpoint_path"] = self.checkpoint_path
        return payload


def load_request(path: Path) -> SurrogateRequest:
    raw = json.loads(path.read_text())
    return SurrogateRequest.from_mapping(raw)


def write_response(path: Path, response: SurrogateResponse) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(response.to_mapping(), indent=2, sort_keys=True) + "\n")
