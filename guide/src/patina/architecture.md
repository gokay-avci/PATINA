# 6.2 Architecture

The working architectural picture is:

```mermaid
flowchart TB
    %% Editing guide:
    %% - Use one subgraph per conceptual lane.
    %% - Put the main evidence flow in the edge block, then keep the class assignments grouped at the end.
    classDef legacy fill:#fbf7ef,stroke:#8a7453,color:#172328,stroke-width:2px;
    classDef seam fill:#f5f7f9,stroke:#5f7380,color:#172328,stroke-width:2px;
    classDef modern fill:#f4f8f7,stroke:#55736d,color:#172328,stroke-width:2px;
    classDef research fill:#ffffff,stroke:#7a7a7a,color:#172328,stroke-width:1.8px;

    subgraph LEGACY["Legacy KLMC3 / SCOTT"]
        LCTRL["Workflow control<br/>SCOTT.f90 / Master.f90"]
        LSEARCH["Native search workflows<br/>GA / BH / annealing / thresholds"]
        LEVAL["Native staged evaluator semantics"]
        LFILES["Template and filesystem conventions"]
    end

    subgraph SEAMS["Migration And Evidence Seams"]
        FIX["Preserved fixtures and provenance audits"]
        PARITY["Scott-shaped kernels and typed parity decisions"]
        IMPORT["Explicit adapter boundaries and imports"]
    end

    subgraph MODERN["PATINA"]
        TYPES["patina-types<br/>transport, manifests, checkpoints"]
        SCI["patina-sci-kernel<br/>validated scientific semantics"]
        SEARCH["patina-search<br/>controllers, kernels, move logic"]
        DRIVER["patina-driver<br/>workflow orchestration"]
        EXT["patina-external / patina-evaluator / patina-runtime<br/>execution and parsing"]
        TOPO["Topology identity path<br/>dreadnaut and hashkeys"]
        ART["Tracked artifacts<br/>CSV / JSON / manifests / checkpoints"]
        UX["CLI / TUI / docs-facing launch surfaces"]
    end

    subgraph RESEARCH["Explicit Research Lanes"]
        EMU["patina-emulate<br/>advisory surrogate lane"]
        SURF["patina-surface / patina-raspa / side science"]
    end

    LCTRL --> FIX
    LSEARCH --> PARITY
    LEVAL --> IMPORT
    LFILES --> IMPORT

    FIX --> TYPES
    PARITY --> SEARCH
    IMPORT --> EXT

    TYPES --> SCI
    SCI --> SEARCH
    SEARCH --> DRIVER
    DRIVER --> EXT
    DRIVER --> TOPO
    DRIVER --> ART
    UX --> DRIVER
    DRIVER --> EMU
    DRIVER --> SURF

    class LCTRL,LSEARCH,LEVAL,LFILES legacy;
    class FIX,PARITY,IMPORT seam;
    class TYPES,SCI,SEARCH,DRIVER,EXT,TOPO,ART,UX modern;
    class EMU,SURF research;
```

The practical boundary rules are:

- `patina-types` carries transport and checkpoint data
- `patina-sci-kernel` owns validated scientific structure semantics
- adapter crates own external-program specifics and subprocess behavior
- application crates own orchestration
- research lanes such as `patina-emulate` stay explicit instead of being hidden inside parity-core services

> [!NOTE]
> Readers approaching from the scientific side should read this page together with
> [6.1 Hexagonal Ports And Adapters](../software-architecture/hexagonal-architecture.md).
> The same boundary that protects software maintainability also protects scientific semantics from
> being mixed with file-format and subprocess details.

## Why This Diagram Is Larger Than Before

The architecture is not only a clean-room Rust design. It is a migration from a historically rich
Fortran workflow stack into a more explicit system of kernels, services, and adapters. The diagram
therefore needs to show four things at once:

- where KLMC3 meaning came from
- which seams preserve or audit that meaning
- how PATINA separates core semantics from orchestration and adapters
- where research lanes such as emulation sit without contaminating parity-core behavior

The public book should still explain these boundaries with small, testable examples rather than
giant code dumps, but the map itself has to admit the real migration complexity.
