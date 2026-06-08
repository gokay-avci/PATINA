<script>
  import { onDestroy, onMount } from 'svelte';

  export let structure = null;
  export let title = 'Structure';
  export let context = '';
  export let height = 360;
  export let compact = false;
  export let selectedSites = [];
  export let startInfoOpen = true;

  let host;
  let StructureComponent = null;
  let loading = false;
  let error = '';
  let observer = null;
  let controlsOpen = false;
  let infoPaneOpen = startInfoOpen;
  let showImageAtoms = false;
  let showBonds = 'never';
  let colorScheme = 'Vesta';
  let performanceMode = 'speed';
  let supercellScaling = '1x1x1';
  let sphereSegments = compact ? 8 : 12;
  let atomColorConfig = { mode: 'element', scale: 'Viridis', scale_type: 'categorical' };

  $: siteCount = structure?.sites?.length ?? 0;
  $: lattice = structure?.lattice;
  $: formula = formulaFromStructure(structure);
  $: displayStructure = centeredStructureForDisplay(structure);
  $: sceneProps = {
    background_opacity: 0,
    show_bonds: showBonds,
    sphere_segments: Number(sphereSegments) || 8
  };

  function formulaFromStructure(value) {
    const counts = new Map();
    for (const site of value?.sites ?? []) {
      const element = site?.species?.[0]?.element ?? 'X';
      counts.set(element, (counts.get(element) ?? 0) + 1);
    }
    return Array.from(counts.entries())
      .map(([element, count]) => `${element}${count > 1 ? count : ''}`)
      .join('');
  }

  function centeredStructureForDisplay(value) {
    const sites = value?.sites ?? [];
    const pbc = value?.lattice?.pbc ?? [false, false, false];
    if (!sites.length || pbc.some(Boolean)) return value;

    const coords = sites.map((site) => site.xyz ?? [0, 0, 0]);
    const mins = [0, 1, 2].map((axis) => Math.min(...coords.map((coord) => coord[axis])));
    const maxs = [0, 1, 2].map((axis) => Math.max(...coords.map((coord) => coord[axis])));
    const center = [0, 1, 2].map((axis) => (mins[axis] + maxs[axis]) / 2);
    const span = Math.max(...[0, 1, 2].map((axis) => maxs[axis] - mins[axis]), 0);
    const box = Math.max(span + 6, 8);
    const shiftedSites = sites.map((site) => {
      const xyz = (site.xyz ?? [0, 0, 0]).map((value, axis) => value - center[axis] + box / 2);
      return {
        ...site,
        xyz,
        abc: xyz.map((value) => value / box)
      };
    });

    return {
      ...value,
      sites: shiftedSites,
      lattice: {
        matrix: [[box, 0, 0], [0, box, 0], [0, 0, box]],
        a: box,
        b: box,
        c: box,
        alpha: 90,
        beta: 90,
        gamma: 90,
        volume: box ** 3,
        pbc: [false, false, false]
      }
    };
  }

  async function loadMatterViz() {
    if (StructureComponent || loading) return;
    loading = true;
    error = '';
    try {
      const module = await import('matterviz/structure');
      StructureComponent = module.Structure;
    } catch (err) {
      error = `${err}`;
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    if (!host || typeof IntersectionObserver === 'undefined') {
      void loadMatterViz();
      return;
    }
    observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          void loadMatterViz();
          observer?.disconnect();
        }
      },
      { rootMargin: '220px' }
    );
    observer.observe(host);
  });

  onDestroy(() => {
    observer?.disconnect();
  });
</script>

<div class="matterviz-frame" class:compact bind:this={host}>
  <div class="matterviz-toolbar">
    <div>
      <span>{context}</span>
      <strong>{title}</strong>
    </div>
    <div class="matterviz-options">
      <label>
        Info
        <input type="checkbox" bind:checked={infoPaneOpen} />
      </label>
      <label>
        Controls
        <input type="checkbox" bind:checked={controlsOpen} />
      </label>
      <label>
        Images
        <input type="checkbox" bind:checked={showImageAtoms} />
      </label>
      <label>
        Bonds
        <select bind:value={showBonds}>
          <option value="never">off</option>
          <option value="always">on</option>
          <option value="crystals">crystal</option>
          <option value="molecules">molecule</option>
        </select>
      </label>
      <label>
        Color
        <select bind:value={colorScheme}>
          <option value="Vesta">VESTA</option>
          <option value="Jmol">Jmol</option>
        </select>
      </label>
      <label>
        Supercell
        <select bind:value={supercellScaling}>
          <option value="1x1x1">1x1x1</option>
          <option value="2x1x1">2x1x1</option>
          <option value="2x2x1">2x2x1</option>
          <option value="2x2x2">2x2x2</option>
        </select>
      </label>
      <label>
        Mode
        <select bind:value={performanceMode}>
          <option value="speed">speed</option>
          <option value="quality">quality</option>
        </select>
      </label>
    </div>
  </div>

  <div class="matterviz-stage" style={`height:${height}px;`}>
    {#if displayStructure && StructureComponent}
      <svelte:component
        this={StructureComponent}
        structure={displayStructure}
        height={height}
        show_controls={controlsOpen}
        enable_info_pane={true}
        bind:info_pane_open={infoPaneOpen}
        performance_mode={performanceMode}
        selected_sites={selectedSites}
        show_image_atoms={showImageAtoms}
        supercell_scaling={supercellScaling}
        color_scheme={colorScheme}
        atom_color_config={atomColorConfig}
        scene_props={sceneProps}
      />
    {:else if error}
      <div class="placeholder">{error}</div>
    {:else if structure}
      <div class="placeholder">Loading 3D viewer...</div>
    {:else}
      <div class="placeholder">No structure payload is available.</div>
    {/if}
  </div>

  <div class="matterviz-meta">
    <span>{formula || '-'}</span>
    <span>{siteCount} sites</span>
    <span>{lattice?.pbc?.filter(Boolean).length ?? 0}D periodic</span>
    {#if lattice?.a}<span>a {lattice.a.toFixed(2)} A</span>{/if}
    {#if lattice?.volume}<span>V {lattice.volume.toFixed(1)} A3</span>{/if}
  </div>
</div>
