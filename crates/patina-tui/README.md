# patina-tui

Terminal UI for browsing and diagnosing PATINA run artifacts.

Current scope:

- load an existing run directory
- inspect GA generation metrics
- inspect controller trace rows
- inspect typed generation snapshots from `raw/generation_XXXX_state.json`
- browse discovered raw and output artifacts

Example:

```bash
cargo run -p patina-tui -- runs/active/new_runs/staged_scott_ga_mgo24_gulp_realistic
cargo run -p patina-tui -- --follow runs/active/new_runs/staged_scott_ga_mgo24_gulp_realistic
```

Keybindings:

- `q`: quit
- `tab`: cycle screen
- `up` / `k`: move selection up
- `down` / `j`: move selection down
- `left` / `h`: move member context (generation/workbench)
- `right` / `l`: move member context (generation/workbench)
- `r`: reload artifacts from disk
- `f`: toggle follow mode
- `1`: dashboard
- `2`: generation inspector
- `3`: artifact browser
- `4`: architecture
- `5`: flow
- `6`: workbench
- `7`: launch
- `8`: ports
- `?`: help overlay

Flow layering keys:

- `u`: focus upstream pane
- `c`: focus current pane
- `d`: focus downstream pane
- `n`: focus actions pane
- `m`: cycle focused pane
- `enter`: open linked workspace for focused pane

Ports interactivity keys:

- `a`: toggle all seams vs relevant seams
- `v`: cycle detail mode (contract, evidence, actions)
- `x`: run seam probe checks
- `g`: open launch workspace prefilled from selected seam
- `enter`: open linked workspace

Deferred (later todo):

- richer card stack variants (risk card, metrics card, policy card)
- animated transitions between pane layers
- advanced visual theming passes not required for core workflow utility
- deep provenance expansion beyond current seam/workflow cards
