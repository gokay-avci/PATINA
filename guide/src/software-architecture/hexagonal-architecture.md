# 6.1 Hexagonal Ports And Adapters

Hexagonal architecture, also called ports and adapters, was introduced by Alistair Cockburn to keep
core logic independent from external technologies @cockburn2005.

In `patina`, that pattern is not only a software-quality preference. It is also a scientific safety
mechanism.

{{#tabs global="lens"}}
{{#tab name="Software Lens"}}

The software view is the classic one:

- the core owns semantics and use cases
- ports define what the outside world must provide
- adapters translate files, CLIs, Python processes, and external codes into those ports

That separation improves testability, reduces incidental coupling, and makes it practical to swap
or extend evaluators without rewriting the core controller logic.

{{#endtab}}
{{#tab name="Materials Lens"}}

The materials-science consequence is more specific:

- typed structure semantics should not be owned by file templates
- scientific transforms should not be hidden inside subprocess wrappers
- external-program quirks should not silently redefine the meaning of the search state

If those boundaries blur, the project stops being auditable. Readers can no longer tell whether a
behavior comes from the intended scientific model or from an accidental backend convention.

{{#endtab}}
{{#tab name="Bridge"}}

For this project, a useful working picture is:

```mermaid
flowchart TB
    %% Editing guide:
    %% - Keep each subgraph as one architectural lane.
    %% - Add new nodes inside the correct lane, then wire edges below, then update the class line at the end.
    classDef core fill:#f4f8f7,stroke:#55736d,color:#172328,stroke-width:2px;
    classDef app fill:#f5f7f9,stroke:#5f7380,color:#172328,stroke-width:2px;
    classDef port fill:#fbf7ef,stroke:#8a7453,color:#172328,stroke-width:2px;
    classDef adapter fill:#ffffff,stroke:#7a7a7a,color:#172328,stroke-width:1.6px;

    subgraph CORE["Scientific Core"]
        SCI["Scientific semantics<br/>patina-sci-kernel"]
        SEARCH["Search rules and controllers<br/>patina-search"]
        TYPES["Typed records and provenance<br/>patina-types"]
    end

    subgraph APPLICATION["Application Layer"]
        ORCH["Workflow orchestration<br/>patina-driver"]
        UX["CLI / TUI / docs-facing launch surfaces"]
    end

    subgraph PORTS["Ports"]
        EPORT["EvaluatorPort"]
        TPORT["TopologyIdentityPort"]
        FPORT["FeatureProjectionPort"]
        SPORT["SurrogateScoringPort"]
        CPORT["CodecPort"]
        APORT["ArtifactSinkPort"]
    end

    subgraph ADAPTERS["Adapters"]
        EVAL["External evaluators<br/>GULP / Janus / future DFT"]
        TOPO["Topology identity<br/>dreadnaut / hashkey"]
        FEAT["Feature projection<br/>statistics / pair signatures / SOAP"]
        SURR["Python surrogate runtime<br/>patina-emulate/python"]
        CODEC["File and structure codecs"]
        STORE["Run directories and tracked artifacts"]
    end

    SCI --> ORCH
    SEARCH --> ORCH
    TYPES --> ORCH
    UX --> ORCH

    ORCH --> EPORT
    ORCH --> TPORT
    ORCH --> FPORT
    ORCH --> SPORT
    ORCH --> CPORT
    ORCH --> APORT

    EPORT --> EVAL
    TPORT --> TOPO
    FPORT --> FEAT
    SPORT --> SURR
    CPORT --> CODEC
    APORT --> STORE

    class SCI,SEARCH,TYPES core;
    class ORCH,UX app;
    class EPORT,TPORT,FPORT,SPORT,CPORT,APORT port;
    class EVAL,TOPO,FEAT,SURR,CODEC,STORE adapter;
```

The bridge principle is:

- keep the science inside the semantic core
- keep the orchestration inside the application layer
- let adapters do translation work, not policy work

That is why the software and science narratives belong together here instead of in isolated books.

{{#endtab}}
{{#endtabs}}

## Expanded Reading

The point of the hexagonal model in `patina` is not diagram aesthetics. It is to keep several
high-risk boundaries under control at once:

- exact scientific semantics versus transport records
- deterministic workflow logic versus external process behavior
- parity-core workflows versus advisory research lanes such as emulation
- user-facing launch surfaces versus internal persistence and artifact layout

That is why the diagram has more than one adapter type. Evaluators, codecs, topology identity,
feature projectors, and surrogate runtimes all deserve separate ports because they fail for
different reasons and carry different scientific risks.
