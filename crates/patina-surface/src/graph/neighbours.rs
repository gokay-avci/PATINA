// Source-to-target mapping for this module:
// - `to_integrate_project/crystal_surface_generator/src/core/neighbours.rs`
// - `to_integrate_project/crystal_surface_generator/src/core/voronoi_approx.rs`

use crate::domain::{SurfaceInterfaceError, SurfaceSlab};
use crate::graph::voronoi_approx::{compute_weights_for_atom, WeightConfig};
use crate::graph::{shortest_distance_vector, wrap_fractional_axis, wrap_index};
use nalgebra::Vector3;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub(crate) struct Neighbour {
    pub(crate) j: usize,
    pub(crate) r: f64,
    pub(crate) mic: Vector3<f64>,
    pub(crate) weight: f64,
}

#[derive(Debug, Clone)]
pub(crate) struct NeighbourList {
    pub(crate) k: usize,
    pub(crate) neighbours: Vec<Vec<Neighbour>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct NeighbourConfig {
    pub(crate) k: usize,
    pub(crate) max_radius: Option<f64>,
    pub(crate) target_bin_cart: Option<f64>,
    pub(crate) mutual: bool,
    pub(crate) voronoi_weights: bool,
    pub(crate) surface_refine: bool,
    pub(crate) surface_fraction: f64,
    pub(crate) weight_cfg: WeightConfig,
}

impl Default for NeighbourConfig {
    fn default() -> Self {
        Self {
            k: 32,
            max_radius: None,
            target_bin_cart: None,
            mutual: true,
            voronoi_weights: true,
            surface_refine: false,
            surface_fraction: 0.05,
            weight_cfg: WeightConfig::default(),
        }
    }
}

pub(crate) fn build_knn_neighbour_list(
    slab: &SurfaceSlab,
    config: &NeighbourConfig,
) -> Result<NeighbourList, SurfaceInterfaceError> {
    let atom_count = slab.atoms.len();
    if atom_count == 0 {
        return Ok(NeighbourList {
            k: config.k,
            neighbours: Vec::new(),
        });
    }
    if config.k == 0 {
        return Err(SurfaceInterfaceError::GraphKernel("k must be > 0".into()));
    }
    if !(0.0 < config.surface_fraction && config.surface_fraction < 0.5) {
        return Err(SurfaceInterfaceError::GraphKernel(
            "surface_fraction must be in (0,0.5)".into(),
        ));
    }

    let max_radius = config
        .max_radius
        .unwrap_or_else(|| default_global_radius(slab));
    if !(max_radius.is_finite() && max_radius > 0.0) {
        return Err(SurfaceInterfaceError::GraphKernel(format!(
            "invalid max_radius: {max_radius}"
        )));
    }

    let target_bin_cart = config.target_bin_cart.unwrap_or(max_radius);
    let grid = FractionalGrid::new(slab, target_bin_cart, max_radius)?;

    let mut raw = vec![Vec::<(usize, f64, Vector3<f64>)>::new(); atom_count];
    for (i, atom) in slab.atoms.iter().enumerate() {
        let mut candidates = Vec::new();
        for &j in &grid.query(atom.fractional) {
            if i == j {
                continue;
            }
            let dv = shortest_distance_vector(slab, atom.fractional, slab.atoms[j].fractional);
            let r2 = dv.norm_squared();
            if r2 <= max_radius * max_radius {
                candidates.push((j, r2.sqrt(), dv));
            }
        }
        candidates.sort_by(|left, right| left.1.total_cmp(&right.1));
        candidates.truncate(config.k);
        raw[i] = candidates;
    }

    let mut keep_sets = vec![HashSet::<usize>::new(); atom_count];
    if config.mutual {
        for i in 0..atom_count {
            for (j, _, _) in &raw[i] {
                keep_sets[i].insert(*j);
            }
        }
    }

    let mut mutual_mask = vec![HashSet::<usize>::new(); atom_count];
    if config.mutual {
        for i in 0..atom_count {
            for (j, _, _) in &raw[i] {
                if keep_sets[*j].contains(&i) {
                    mutual_mask[i].insert(*j);
                }
            }
        }
    }

    let surface_flags = if config.surface_refine {
        mark_surface_atoms_fractional_z(slab, config.surface_fraction)
    } else {
        vec![false; atom_count]
    };

    let mut neighbours = vec![Vec::<Neighbour>::new(); atom_count];
    for i in 0..atom_count {
        let mut accepted = Vec::new();
        for (j, r, mic) in &raw[i] {
            if config.mutual && !mutual_mask[i].contains(j) {
                continue;
            }
            accepted.push((*j, *r, *mic));
        }

        let weights = if config.voronoi_weights {
            compute_weights_for_atom(slab, i, &accepted, surface_flags[i], &config.weight_cfg)
        } else {
            let r0 = max_radius.max(1.0e-6);
            accepted
                .iter()
                .map(|(_, r, _)| (-(r / r0).powi(2)).exp())
                .collect()
        };

        neighbours[i] = accepted
            .into_iter()
            .zip(weights)
            .map(|((j, r, mic), weight)| Neighbour { j, r, mic, weight })
            .collect();
    }

    Ok(NeighbourList {
        k: config.k,
        neighbours,
    })
}

fn mark_surface_atoms_fractional_z(slab: &SurfaceSlab, surface_fraction: f64) -> Vec<bool> {
    let atom_count = slab.atoms.len();
    let mut indices = (0..atom_count).collect::<Vec<_>>();
    indices.sort_by(|left, right| {
        slab.atoms[*left].fractional[2].total_cmp(&slab.atoms[*right].fractional[2])
    });

    let cutoff = ((atom_count as f64) * surface_fraction).ceil().max(1.0) as usize;
    let mut flags = vec![false; atom_count];
    for &i in indices.iter().take(cutoff) {
        flags[i] = true;
    }
    for &i in indices.iter().rev().take(cutoff) {
        flags[i] = true;
    }
    flags
}

#[derive(Debug)]
struct FractionalGrid {
    nx: i32,
    ny: i32,
    nz: i32,
    sx: i32,
    sy: i32,
    sz: i32,
    buckets: HashMap<(i32, i32, i32), Vec<usize>>,
    periodic_axes: [bool; 3],
}

impl FractionalGrid {
    fn new(
        slab: &SurfaceSlab,
        target_bin_cart: f64,
        max_radius: f64,
    ) -> Result<Self, SurfaceInterfaceError> {
        let a = Vector3::new(slab.lattice[0][0], slab.lattice[1][0], slab.lattice[2][0]).norm();
        let b = Vector3::new(slab.lattice[0][1], slab.lattice[1][1], slab.lattice[2][1]).norm();
        let c = Vector3::new(slab.lattice[0][2], slab.lattice[1][2], slab.lattice[2][2]).norm();
        if a <= 1.0e-12 || b <= 1.0e-12 || c <= 1.0e-12 {
            return Err(SurfaceInterfaceError::GraphKernel(
                "degenerate lattice vectors; cannot build neighbour grid".into(),
            ));
        }

        let bx = (target_bin_cart / a).clamp(1.0e-4, 1.0);
        let by = (target_bin_cart / b).clamp(1.0e-4, 1.0);
        let bz = (target_bin_cart / c).clamp(1.0e-4, 1.0);

        let nx = (1.0 / bx).ceil() as i32;
        let ny = (1.0 / by).ceil() as i32;
        let nz = (1.0 / bz).ceil() as i32;

        let nx = nx.clamp(1, 4096);
        let ny = ny.clamp(1, 4096);
        let nz = nz.clamp(1, 4096);

        let sx = ((max_radius / (a / nx as f64)).ceil() as i32).clamp(1, 16);
        let sy = ((max_radius / (b / ny as f64)).ceil() as i32).clamp(1, 16);
        let sz = ((max_radius / (c / nz as f64)).ceil() as i32).clamp(1, 16);

        let mut buckets = HashMap::<(i32, i32, i32), Vec<usize>>::new();
        buckets.reserve(slab.atoms.len() * 2);
        for (i, atom) in slab.atoms.iter().enumerate() {
            let key = frac_key(atom.fractional, slab.periodic_axes, nx, ny, nz);
            buckets.entry(key).or_default().push(i);
        }

        Ok(Self {
            nx,
            ny,
            nz,
            sx,
            sy,
            sz,
            buckets,
            periodic_axes: slab.periodic_axes,
        })
    }

    fn query(&self, fractional: [f64; 3]) -> Vec<usize> {
        let (ix, iy, iz) = frac_key(fractional, self.periodic_axes, self.nx, self.ny, self.nz);
        let mut out = Vec::new();
        let mut seen = HashSet::<usize>::new();

        for dx in -self.sx..=self.sx {
            let maybe_x = axis_bucket(ix, dx, self.nx, self.periodic_axes[0]);
            if maybe_x.is_none() {
                continue;
            }
            for dy in -self.sy..=self.sy {
                let maybe_y = axis_bucket(iy, dy, self.ny, self.periodic_axes[1]);
                if maybe_y.is_none() {
                    continue;
                }
                for dz in -self.sz..=self.sz {
                    let maybe_z = axis_bucket(iz, dz, self.nz, self.periodic_axes[2]);
                    if let (Some(x), Some(y), Some(z)) = (maybe_x, maybe_y, maybe_z) {
                        if let Some(bucket) = self.buckets.get(&(x, y, z)) {
                            for &atom_index in bucket {
                                if seen.insert(atom_index) {
                                    out.push(atom_index);
                                }
                            }
                        }
                    }
                }
            }
        }

        out
    }
}

fn frac_key(
    fractional: [f64; 3],
    periodic_axes: [bool; 3],
    nx: i32,
    ny: i32,
    nz: i32,
) -> (i32, i32, i32) {
    let x = wrap_fractional_axis(fractional[0], periodic_axes[0]);
    let y = wrap_fractional_axis(fractional[1], periodic_axes[1]);
    let z = wrap_fractional_axis(fractional[2], periodic_axes[2]);
    (
        bucket_index(x, nx),
        bucket_index(y, ny),
        bucket_index(z, nz),
    )
}

fn bucket_index(value: f64, bucket_count: i32) -> i32 {
    let scaled = (value * bucket_count as f64).floor() as i32;
    scaled.clamp(0, bucket_count - 1)
}

fn axis_bucket(current: i32, delta: i32, bucket_count: i32, periodic: bool) -> Option<i32> {
    let next = current + delta;
    if periodic {
        Some(wrap_index(next, bucket_count))
    } else if (0..bucket_count).contains(&next) {
        Some(next)
    } else {
        None
    }
}

fn default_global_radius(slab: &SurfaceSlab) -> f64 {
    let a = Vector3::new(slab.lattice[0][0], slab.lattice[1][0], slab.lattice[2][0]).norm();
    let b = Vector3::new(slab.lattice[0][1], slab.lattice[1][1], slab.lattice[2][1]).norm();
    let c = Vector3::new(slab.lattice[0][2], slab.lattice[1][2], slab.lattice[2][2]).norm();
    (0.35 * a.max(b).max(c)).clamp(3.0, 8.0)
}
