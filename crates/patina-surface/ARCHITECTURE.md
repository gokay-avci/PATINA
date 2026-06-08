# patina-surface Architecture Note

`patina-surface` is being ported as a library-first surface-science crate, not as a CLI-shaped copy
of `to_integrate_project/crystal_surface_generator`.

Current internal ownership:

- `domain`: stable typed contracts shared with `patina-driver` and future adapters
- `generation`: slab geometry and population kernels
- `cut`: cut-selection logic, currently heuristic topology-aware void crawling
- `reduction`: slab dedup plus the explicitly scaffolded in-plane reduction lane
- `graph`: bonding, connectivity, neighbour, and Voronoi-approx support
- `analysis`: diagnostics and surface-bond analysis over the graph layer
- `engine`: orchestration over the internal kernels and exported diagnostics/polarity payloads
- `math`: exact integer and reduction helpers imported from the upstream project

Planned next ownership slices:

- `reconstruction`: surface-reconstruction support only where it belongs in the library layer;
  the primary Monte-Carlo workflow shape lives in `patina-driver`
- `io`: optional fixture/adaptor IO helpers
- `chemistry`: optional, feature-gated chemistry/MOFid integration

Design rule:

- scientific kernels live in internal modules
- orchestration lives in engines and adapters
- anything heuristic or scaffolded must remain labeled as such
