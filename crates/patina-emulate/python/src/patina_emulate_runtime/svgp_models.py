from __future__ import annotations

import gpytorch
import numpy as np
import torch
from pathlib import Path
from autoemulate.core.device import TorchDeviceMixin
from autoemulate.emulators import register
from autoemulate.emulators.base import ProbabilisticEmulator
from torch.utils.data import DataLoader, TensorDataset


DEFAULT_SEED = 42


def _metadata_compatible(expected: dict | None, actual: dict | None) -> bool:
    if not expected or not actual:
        return False
    for key in ("model_variant", "feature_count", "target_count", "num_inducing"):
        if expected.get(key) != actual.get(key):
            return False
    if "actual_num_inducing" in expected and "actual_num_inducing" in actual:
        if expected.get("actual_num_inducing") != actual.get("actual_num_inducing"):
            return False
    return True


def _torch_load_checkpoint(path: Path, device):
    try:
        return torch.load(path, map_location=device, weights_only=False)
    except TypeError:
        return torch.load(path, map_location=device)


def _load_checkpoint(
    model,
    likelihood,
    checkpoint_path: Path | None,
    checkpoint_metadata: dict | None,
    device,
):
    if checkpoint_path is None or not checkpoint_path.exists():
        return None
    payload = _torch_load_checkpoint(checkpoint_path, device)
    if not isinstance(payload, dict):
        return None
    if not _metadata_compatible(checkpoint_metadata, payload.get("metadata")):
        return None
    try:
        model.load_state_dict(payload["model_state_dict"])
        likelihood.load_state_dict(payload["likelihood_state_dict"])
    except RuntimeError:
        return None
    return payload


def _try_load_optimizer_state(optimizer, payload):
    if not isinstance(payload, dict):
        return
    optimizer_state = payload.get("optimizer_state_dict")
    if optimizer_state is None:
        return
    try:
        optimizer.load_state_dict(optimizer_state)
    except (ValueError, RuntimeError):
        return


def _save_checkpoint(
    checkpoint_path: Path | None,
    model,
    likelihood,
    optimizer,
    checkpoint_metadata: dict | None,
):
    if checkpoint_path is None:
        return
    checkpoint_path.parent.mkdir(parents=True, exist_ok=True)
    torch.save(
        {
            "metadata": dict(checkpoint_metadata or {}),
            "model_state_dict": model.state_dict(),
            "likelihood_state_dict": likelihood.state_dict(),
            "optimizer_state_dict": optimizer.state_dict(),
        },
        checkpoint_path,
    )


class CoreSVGP(gpytorch.models.ApproximateGP):
    def __init__(
        self,
        inducing_points: torch.Tensor,
        variational_distribution_cls,
        variational_strategy_cls,
    ):
        variational_distribution = variational_distribution_cls(inducing_points.size(0))
        variational_strategy = variational_strategy_cls(
            self,
            inducing_points,
            variational_distribution,
            learn_inducing_locations=True,
        )
        super().__init__(variational_strategy)
        self.mean_module = gpytorch.means.ConstantMean()
        self.covar_module = gpytorch.kernels.ScaleKernel(gpytorch.kernels.MaternKernel(nu=2.5))

    def forward(self, x: torch.Tensor):
        mean_x = self.mean_module(x)
        covar_x = self.covar_module(x)
        return gpytorch.distributions.MultivariateNormal(mean_x, covar_x)


class BaseSVGP(ProbabilisticEmulator):
    supports_uq = True

    def __init__(
        self,
        x,
        y,
        num_inducing: int = 24,
        epochs: int = 250,
        batch_size: int = 32,
        lr: float = 0.05,
        device: str | torch.device | None = None,
        random_state: int = DEFAULT_SEED,
        checkpoint_path: str | None = None,
        checkpoint_resume: bool = True,
        checkpoint_save: bool = True,
        checkpoint_metadata: dict | None = None,
    ):
        TorchDeviceMixin.__init__(self, device=device)

        x, y = self._convert_to_tensors(x, y)
        x, y = self._move_tensors_to_device(x, y)

        self.num_inducing = num_inducing
        self.epochs = epochs
        self.batch_size = batch_size
        self.lr = lr
        self.random_state = random_state
        self.checkpoint_path = Path(checkpoint_path) if checkpoint_path else None
        self.checkpoint_resume = checkpoint_resume
        self.checkpoint_save = checkpoint_save
        self.checkpoint_metadata = dict(checkpoint_metadata or {})

        self.x_transform = None
        self.y_transform = None

        inducing_points = self._select_inducing_points(x)
        self.checkpoint_metadata["actual_num_inducing"] = int(inducing_points.size(0))
        self.model = self._build_core_model(inducing_points).to(self.device)
        self.likelihood = gpytorch.likelihoods.GaussianLikelihood().to(self.device)
        self._checkpoint_payload = (
            _load_checkpoint(
                self.model,
                self.likelihood,
                self.checkpoint_path,
                self.checkpoint_metadata,
                self.device,
            )
            if self.checkpoint_resume
            else None
        )

    @classmethod
    def model_name(cls) -> str:
        return cls.__name__

    def _build_core_model(self, inducing_points: torch.Tensor):
        raise NotImplementedError

    def _select_inducing_points(self, x: torch.Tensor) -> torch.Tensor:
        num_inducing = min(self.num_inducing, x.size(0))
        generator = torch.Generator(device=x.device)
        generator.manual_seed(self.random_state)
        indices = torch.randperm(x.size(0), generator=generator, device=x.device)[:num_inducing]
        return x[indices].clone()

    def _fit(self, x: torch.Tensor, y: torch.Tensor):
        self.model.train()
        self.likelihood.train()

        y_flat = y.squeeze(-1)
        dataset = TensorDataset(x, y_flat)
        loader = DataLoader(
            dataset,
            batch_size=min(self.batch_size, len(dataset)),
            shuffle=True,
        )

        optimizer = torch.optim.Adam(
            list(self.model.parameters()) + list(self.likelihood.parameters()),
            lr=self.lr,
        )
        _try_load_optimizer_state(optimizer, self._checkpoint_payload)
        mll = gpytorch.mlls.VariationalELBO(self.likelihood, self.model, num_data=x.size(0))

        for _ in range(self.epochs):
            for x_batch, y_batch in loader:
                optimizer.zero_grad()
                output = self.model(x_batch)
                loss = -mll(output, y_batch)
                loss.backward()
                optimizer.step()
        if self.checkpoint_save:
            _save_checkpoint(
                self.checkpoint_path,
                self.model,
                self.likelihood,
                optimizer,
                self.checkpoint_metadata,
            )
        return self

    def _predict(self, x: torch.Tensor, with_grad: bool):
        self.model.eval()
        self.likelihood.eval()
        with torch.set_grad_enabled(with_grad), gpytorch.settings.fast_pred_var():
            posterior = self.likelihood(self.model(x.to(self.device)))
            mean = posterior.mean.unsqueeze(-1)
            covariance = posterior.variance.clamp_min(1e-9).view(-1, 1, 1)
            return gpytorch.distributions.MultivariateNormal(mean, covariance)

    @staticmethod
    def is_multioutput() -> bool:
        return False

    @staticmethod
    def get_tune_params():
        return {
            "num_inducing": [16, 24, 32],
            "epochs": [150, 250],
            "batch_size": [16, 32],
            "lr": [0.03, 0.05],
        }


@register(overwrite=True)
class MeanFieldSVGP(BaseSVGP):
    @classmethod
    def model_name(cls) -> str:
        return "Mean-Field SVGP"

    def _build_core_model(self, inducing_points: torch.Tensor):
        return CoreSVGP(
            inducing_points=inducing_points,
            variational_distribution_cls=gpytorch.variational.MeanFieldVariationalDistribution,
            variational_strategy_cls=gpytorch.variational.VariationalStrategy,
        )


@register(overwrite=True)
class UnwhitenedSVGP(BaseSVGP):
    @classmethod
    def model_name(cls) -> str:
        return "Unwhitened Full-Covariance SVGP"

    def _build_core_model(self, inducing_points: torch.Tensor):
        return CoreSVGP(
            inducing_points=inducing_points,
            variational_distribution_cls=gpytorch.variational.CholeskyVariationalDistribution,
            variational_strategy_cls=gpytorch.variational.UnwhitenedVariationalStrategy,
        )


@register(overwrite=True)
class WhitenedSVGP(BaseSVGP):
    @classmethod
    def model_name(cls) -> str:
        return "Whitened Full-Covariance SVGP"

    def _build_core_model(self, inducing_points: torch.Tensor):
        return CoreSVGP(
            inducing_points=inducing_points,
            variational_distribution_cls=gpytorch.variational.CholeskyVariationalDistribution,
            variational_strategy_cls=gpytorch.variational.VariationalStrategy,
        )


def tensor_to_numpy(tensor: torch.Tensor) -> np.ndarray:
    return tensor.detach().cpu().numpy()


def _extract_pointwise_task_gaussian(
    posterior,
    num_points: int,
    num_tasks: int,
):
    mean = posterior.mean.reshape(num_points, num_tasks)
    cov = posterior.covariance_matrix
    cov_blocks = cov.reshape(num_points, num_tasks, num_points, num_tasks)
    task_covs = cov_blocks[torch.arange(num_points), :, torch.arange(num_points), :]
    eye = torch.eye(num_tasks, device=task_covs.device).expand(num_points, -1, -1)
    task_covs = task_covs + 1e-6 * eye
    return gpytorch.distributions.MultivariateNormal(mean, task_covs)


class CoreLMCMultiTaskSVGP(gpytorch.models.ApproximateGP):
    def __init__(
        self,
        inducing_points: torch.Tensor,
        num_tasks: int,
        num_latents: int,
        variational_distribution_cls,
        variational_strategy_cls,
    ):
        batch_shape = torch.Size([num_latents])
        variational_distribution = variational_distribution_cls(
            inducing_points.size(-2),
            batch_shape=batch_shape,
        )
        base_strategy = variational_strategy_cls(
            self,
            inducing_points,
            variational_distribution,
            learn_inducing_locations=True,
        )
        lmc_strategy = gpytorch.variational.LMCVariationalStrategy(
            base_strategy,
            num_tasks=num_tasks,
            num_latents=num_latents,
            latent_dim=-1,
        )
        super().__init__(lmc_strategy)
        self.mean_module = gpytorch.means.ConstantMean(batch_shape=batch_shape)
        self.covar_module = gpytorch.kernels.ScaleKernel(
            gpytorch.kernels.MaternKernel(nu=2.5, batch_shape=batch_shape),
            batch_shape=batch_shape,
        )

    def forward(self, x: torch.Tensor):
        mean_x = self.mean_module(x)
        covar_x = self.covar_module(x)
        return gpytorch.distributions.MultivariateNormal(mean_x, covar_x)


class BaseMultiTaskSVGP(ProbabilisticEmulator):
    supports_uq = True

    def __init__(
        self,
        x,
        y,
        num_inducing: int = 24,
        num_latents: int | None = None,
        epochs: int = 250,
        batch_size: int = 32,
        lr: float = 0.05,
        device: str | torch.device | None = None,
        random_state: int = DEFAULT_SEED,
        checkpoint_path: str | None = None,
        checkpoint_resume: bool = True,
        checkpoint_save: bool = True,
        checkpoint_metadata: dict | None = None,
    ):
        TorchDeviceMixin.__init__(self, device=device)

        x, y = self._convert_to_tensors(x, y)
        x, y = self._move_tensors_to_device(x, y)

        self.num_tasks = y.shape[1]
        self.num_latents = num_latents or self.num_tasks
        self.num_inducing = num_inducing
        self.epochs = epochs
        self.batch_size = batch_size
        self.lr = lr
        self.random_state = random_state
        self.checkpoint_path = Path(checkpoint_path) if checkpoint_path else None
        self.checkpoint_resume = checkpoint_resume
        self.checkpoint_save = checkpoint_save
        self.checkpoint_metadata = dict(checkpoint_metadata or {})

        self.x_transform = None
        self.y_transform = None

        inducing_points = self._select_inducing_points(x)
        self.checkpoint_metadata["actual_num_inducing"] = int(inducing_points.size(-2))
        self.model = self._build_core_model(inducing_points).to(self.device)
        self.likelihood = gpytorch.likelihoods.MultitaskGaussianLikelihood(
            num_tasks=self.num_tasks
        ).to(self.device)
        self._checkpoint_payload = (
            _load_checkpoint(
                self.model,
                self.likelihood,
                self.checkpoint_path,
                self.checkpoint_metadata,
                self.device,
            )
            if self.checkpoint_resume
            else None
        )

    @classmethod
    def model_name(cls) -> str:
        return cls.__name__

    def _build_core_model(self, inducing_points: torch.Tensor):
        raise NotImplementedError

    def _select_inducing_points(self, x: torch.Tensor) -> torch.Tensor:
        num_inducing = min(self.num_inducing, x.size(0))
        generator = torch.Generator(device=x.device)
        generator.manual_seed(self.random_state)
        indices = torch.randperm(x.size(0), generator=generator, device=x.device)[:num_inducing]
        base_points = x[indices].clone()
        return base_points.unsqueeze(0).repeat(self.num_latents, 1, 1)

    def _fit(self, x: torch.Tensor, y: torch.Tensor):
        self.model.train()
        self.likelihood.train()

        dataset = TensorDataset(x, y)
        loader = DataLoader(
            dataset,
            batch_size=min(self.batch_size, len(dataset)),
            shuffle=True,
        )

        optimizer = torch.optim.Adam(
            list(self.model.parameters()) + list(self.likelihood.parameters()),
            lr=self.lr,
        )
        _try_load_optimizer_state(optimizer, self._checkpoint_payload)
        mll = gpytorch.mlls.VariationalELBO(self.likelihood, self.model, num_data=x.size(0))

        for _ in range(self.epochs):
            for x_batch, y_batch in loader:
                optimizer.zero_grad()
                output = self.model(x_batch)
                loss = -mll(output, y_batch)
                loss.backward()
                optimizer.step()
        if self.checkpoint_save:
            _save_checkpoint(
                self.checkpoint_path,
                self.model,
                self.likelihood,
                optimizer,
                self.checkpoint_metadata,
            )
        return self

    def _predict(self, x: torch.Tensor, with_grad: bool):
        self.model.eval()
        self.likelihood.eval()
        with torch.set_grad_enabled(with_grad), gpytorch.settings.fast_pred_var():
            posterior = self.likelihood(self.model(x.to(self.device)))
            return _extract_pointwise_task_gaussian(
                posterior,
                num_points=x.shape[0],
                num_tasks=self.num_tasks,
            )

    @staticmethod
    def is_multioutput() -> bool:
        return True

    @staticmethod
    def get_tune_params():
        return {
            "num_inducing": [16, 24, 32],
            "epochs": [150, 250],
            "batch_size": [16, 32],
            "lr": [0.03, 0.05],
        }


@register(overwrite=True)
class MultiTaskMeanFieldSVGP(BaseMultiTaskSVGP):
    @classmethod
    def model_name(cls) -> str:
        return "Multitask Mean-Field SVGP"

    def _build_core_model(self, inducing_points: torch.Tensor):
        return CoreLMCMultiTaskSVGP(
            inducing_points=inducing_points,
            num_tasks=self.num_tasks,
            num_latents=self.num_latents,
            variational_distribution_cls=gpytorch.variational.MeanFieldVariationalDistribution,
            variational_strategy_cls=gpytorch.variational.VariationalStrategy,
        )


@register(overwrite=True)
class MultiTaskUnwhitenedSVGP(BaseMultiTaskSVGP):
    @classmethod
    def model_name(cls) -> str:
        return "Multitask Unwhitened Full-Covariance SVGP"

    def _build_core_model(self, inducing_points: torch.Tensor):
        return CoreLMCMultiTaskSVGP(
            inducing_points=inducing_points,
            num_tasks=self.num_tasks,
            num_latents=self.num_latents,
            variational_distribution_cls=gpytorch.variational.CholeskyVariationalDistribution,
            variational_strategy_cls=gpytorch.variational.UnwhitenedVariationalStrategy,
        )


@register(overwrite=True)
class MultiTaskWhitenedSVGP(BaseMultiTaskSVGP):
    @classmethod
    def model_name(cls) -> str:
        return "Multitask Whitened Full-Covariance SVGP"

    def _build_core_model(self, inducing_points: torch.Tensor):
        return CoreLMCMultiTaskSVGP(
            inducing_points=inducing_points,
            num_tasks=self.num_tasks,
            num_latents=self.num_latents,
            variational_distribution_cls=gpytorch.variational.CholeskyVariationalDistribution,
            variational_strategy_cls=gpytorch.variational.VariationalStrategy,
        )
