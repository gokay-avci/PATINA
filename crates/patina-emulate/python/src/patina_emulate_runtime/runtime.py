from __future__ import annotations

import math
from pathlib import Path
from dataclasses import replace

from .contracts import (
    RESPONSE_SCHEMA_VERSION,
    CandidatePrediction,
    SurrogateRequest,
    SurrogateResponse,
)


SINGLE_OUTPUT_MODELS = {
    "mean_field_svgp": "MeanFieldSVGP",
    "unwhitened_svgp": "UnwhitenedSVGP",
    "whitened_svgp": "WhitenedSVGP",
}

MULTITASK_MODELS = {
    "multitask_mean_field_svgp": "MultiTaskMeanFieldSVGP",
    "multitask_unwhitened_svgp": "MultiTaskUnwhitenedSVGP",
    "multitask_whitened_svgp": "MultiTaskWhitenedSVGP",
}


def _resolve_model_class(request: SurrogateRequest):
    from . import svgp_models

    target_count = len(request.target_names)
    model_variant = request.surrogate.model_variant
    if target_count == 1:
        model_name = SINGLE_OUTPUT_MODELS.get(model_variant)
    else:
        model_name = MULTITASK_MODELS.get(model_variant)
    if model_name is None:
        raise ValueError(
            f"model_variant {model_variant!r} is not compatible with target count {target_count}"
        )
    return getattr(svgp_models, model_name)


def _safe_log_level(raw: str) -> str:
    candidate = str(raw).lower()
    if candidate in {"progress_bar", "debug", "info", "warning", "error", "critical"}:
        return candidate
    return "warning"


def _build_autoemulate(request: SurrogateRequest):
    import numpy as np
    from autoemulate import AutoEmulate

    model_class = _resolve_model_class(request)
    x_train = np.asarray([row.features for row in request.training_rows], dtype=float)
    y_train = np.asarray([row.targets for row in request.training_rows], dtype=float)
    if y_train.shape[1] == 1:
        y_train = y_train[:, 0]

    extra_kwargs = {
        "n_splits": max(2, min(3, len(request.training_rows))),
    }
    if len(request.training_rows) < 6:
        # Early branch histories are often tiny; force an explicit non-empty evaluation split
        # instead of relying on AutoEmulate's internal random splitter.
        extra_kwargs["test_data"] = (x_train, y_train)

    model_params = {
        "epochs": request.surrogate.epochs,
        "num_inducing": request.surrogate.num_inducing,
        "batch_size": request.surrogate.batch_size,
        "lr": request.surrogate.lr,
    }
    checkpoint_path = _checkpoint_path(request)
    if checkpoint_path is not None:
        model_params.update(
            {
                "checkpoint_path": str(checkpoint_path),
                "checkpoint_resume": bool(request.surrogate.checkpoint.resume),
                "checkpoint_save": bool(request.surrogate.checkpoint.save),
                "checkpoint_metadata": _checkpoint_metadata(request),
            }
        )

    return AutoEmulate(
        x=x_train,
        y=y_train,
        models=[model_class],
        model_params=model_params,
        n_bootstraps=request.surrogate.n_bootstraps,
        random_seed=request.surrogate.random_seed,
        log_level=_safe_log_level(request.surrogate.log_level),
        evaluation_metrics=["r2", "rmse"],
        **extra_kwargs,
    )


def _checkpoint_path(request: SurrogateRequest) -> Path | None:
    checkpoint = request.surrogate.checkpoint
    if checkpoint is None:
        return None
    root = Path(checkpoint.checkpoint_dir)
    return root / "svgp_model.pt"


def _checkpoint_metadata(request: SurrogateRequest) -> dict[str, object]:
    return {
        "schema_version": "patina.emulate.svgp_checkpoint.v1",
        "campaign_id": request.campaign_id,
        "branch_id": request.branch_id,
        "workflow": request.workflow,
        "task": request.task,
        "model_variant": request.surrogate.model_variant,
        "feature_names": list(request.feature_names),
        "target_names": list(request.target_names),
        "feature_count": len(request.feature_names),
        "target_count": len(request.target_names),
        "num_inducing": request.surrogate.num_inducing,
    }


def _resolve_predictor(ae):
    if getattr(ae, "results", None):
        result = ae.results[0]
        return result.model, getattr(result, "model_name", type(result.model).__name__)
    if hasattr(ae, "refit_best"):
        best = ae.refit_best()
        model = getattr(best, "best_model", best)
        model_name = getattr(best, "best_model_name", type(model).__name__)
        return model, model_name
    raise RuntimeError("AutoEmulate did not expose a usable fitted model")


def _predict_means_and_variances(model, x_query):
    import numpy as np
    import torch

    x_tensor = torch.tensor(x_query, dtype=torch.float32)
    if hasattr(model, "predict_mean_and_variance"):
        mean, variance = model.predict_mean_and_variance(x_tensor)
        if hasattr(mean, "detach"):
            mean = mean.detach().cpu().numpy()
        if hasattr(variance, "detach"):
            variance = variance.detach().cpu().numpy()
        return np.asarray(mean, dtype=float), np.asarray(variance, dtype=float)
    if hasattr(model, "predict"):
        prediction = model.predict(x_query)
        if isinstance(prediction, tuple) and len(prediction) == 2:
            mean, variance = prediction
            return np.asarray(mean, dtype=float), np.asarray(variance, dtype=float)
        mean = np.asarray(prediction, dtype=float)
        variance = np.zeros_like(mean, dtype=float)
        return mean, variance
    raise RuntimeError(f"Unsupported fitted model type: {type(model).__name__}")


def _normalize_prediction_shapes(request: SurrogateRequest, means, variances):
    import numpy as np

    means = np.asarray(means, dtype=float)
    variances = np.asarray(variances, dtype=float)
    if means.ndim == 1:
        means = means.reshape(-1, 1)
    if variances.ndim == 1:
        variances = variances.reshape(-1, 1)
    if means.shape != variances.shape:
        raise RuntimeError("mean/variance shape mismatch from fitted model")
    if means.shape[0] != len(request.candidate_rows):
        raise RuntimeError("prediction row count does not match candidate_rows")
    if means.shape[1] != len(request.target_names):
        raise RuntimeError("prediction target count does not match target_names")
    return means, np.maximum(variances, 1e-12)


def _incumbent_target(request: SurrogateRequest) -> float:
    values = [row.targets[request.surrogate.primary_target_index] for row in request.training_rows]
    if request.surrogate.objective == "maximize":
        return max(values)
    return min(values)


def _acquisition_score(request: SurrogateRequest, incumbent: float, means, variances) -> tuple[float, float]:
    target_idx = request.surrogate.primary_target_index
    mean_value = float(means[target_idx])
    std_value = math.sqrt(float(variances[target_idx]))
    if request.surrogate.objective == "maximize":
        improvement = mean_value - incumbent
    else:
        improvement = incumbent - mean_value
    uncertainty_score = sum(math.sqrt(float(value)) for value in variances)
    acquisition_score = improvement + 0.5 * std_value
    return acquisition_score, uncertainty_score


def score_candidates(request: SurrogateRequest) -> SurrogateResponse:
    import numpy as np

    ae = _build_autoemulate(request)
    if hasattr(ae, "compare"):
        ae.compare()
    predictor, selected_model_name = _resolve_predictor(ae)

    x_query = np.asarray([row.features for row in request.candidate_rows], dtype=float)
    means, variances = _predict_means_and_variances(predictor, x_query)
    means, variances = _normalize_prediction_shapes(request, means, variances)

    incumbent = _incumbent_target(request)
    scored = []
    for candidate_row, mean_row, variance_row in zip(request.candidate_rows, means, variances):
        acquisition_score, uncertainty_score = _acquisition_score(
            request,
            incumbent,
            mean_row,
            variance_row,
        )
        scored.append(
            CandidatePrediction(
                candidate_id=candidate_row.candidate_id,
                means=tuple(float(value) for value in mean_row),
                variances=tuple(float(value) for value in variance_row),
                acquisition_score=float(acquisition_score),
                uncertainty_score=float(uncertainty_score),
                rank=0,
            )
        )

    ranked = sorted(scored, key=lambda item: item.acquisition_score, reverse=True)
    predictions = tuple(replace(item, rank=index) for index, item in enumerate(ranked, start=1))

    return SurrogateResponse(
        schema_version=RESPONSE_SCHEMA_VERSION,
        campaign_id=request.campaign_id,
        branch_id=request.branch_id,
        workflow=request.workflow,
        task=request.task,
        feature_names=request.feature_names,
        target_names=request.target_names,
        model_variant=request.surrogate.model_variant,
        selected_model_name=selected_model_name,
        incumbent_target=float(incumbent),
        checkpoint_path=str(_checkpoint_path(request)) if _checkpoint_path(request) else None,
        predictions=predictions,
    )
