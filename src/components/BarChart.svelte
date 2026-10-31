<script lang="ts">
  // Vertical bars with optional stacked segments and a dashed target line per bar. Pure SVG.
  import { formatHours } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import { t } from "../lib/i18n.svelte";

  export interface Segment {
    key: string;
    label: string;
    color: string;
    seconds: number;
  }
  export interface Bar {
    label: string;
    full: string;
    segments: Segment[];
    target?: number;
    muted?: boolean;
  }

  let { bars, height = 170, showTargets = true, onselect }: { bars: Bar[]; height?: number; showTargets?: boolean; onselect?: (index: number) => void } = $props();
  let width = $state(600);
  const tf = $derived(store.timeFormat);
  const plotH = $derived(height - 28);
  const max = $derived(Math.max(3600, ...bars.map((b) => Math.max(b.segments.reduce((a, s) => a + s.seconds, 0), showTargets ? (b.target ?? 0) : 0))));
  const ticks = $derived.by(() => {
    const hours = max / 3600;
    const step = hours <= 10 ? 2 : hours <= 25 ? 5 : hours <= 60 ? 10 : hours <= 150 ? 25 : 50;
    const out: number[] = [];
    for (let h = step; h * 3600 <= max; h += step) out.push(h * 3600);
    return out;
  });
  const gap = $derived(bars.length > 40 ? 0.15 : 0.3);
  const labelEvery = $derived(bars.length > 20 ? Math.ceil(bars.length / 16) : 1);
  // Before the container is measured `width` is 0; bars would get negative sizes.
  const barWidth = $derived(Math.max(0, (width - 32) / Math.max(1, bars.length)));
</script>

<div class="wrap" bind:clientWidth={width}>
  <svg viewBox="0 0 {width} {height}" {width} {height} role="img" aria-label={t("chart.bar")}>
    {#each ticks as t (t)}
      {@const y = plotH - (t / max) * plotH}
      <line x1="30" x2={width} y1={y} y2={y} stroke="var(--border)" stroke-dasharray="2 3" />
      <text x="26" y={y + 3} text-anchor="end" font-size="9" fill="var(--text-3)">{t / 3600}h</text>
    {/each}
    {#each width > 48 ? bars : [] as b, i (i)}
      {@const w = barWidth}
      {@const x0 = 32 + i * w}
      {@const total = b.segments.reduce((a, s) => a + s.seconds, 0)}
      <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
      <g onclick={() => onselect?.(i)} style:cursor={onselect ? "pointer" : "default"} opacity={b.muted ? 0.45 : 1}>
        <title>{b.full}: {formatHours(total, tf)}{showTargets && b.target ? t("chart.targetTooltip", { t: formatHours(b.target, tf) }) : ""}</title>
        <rect x={x0} y="0" width={w} height={plotH} fill="transparent" />
        {#each b.segments as s, si (s.key)}
          {@const offset = b.segments.slice(0, si).reduce((a, q) => a + q.seconds, 0)}
          {@const h = (s.seconds / max) * plotH}
          {@const y = plotH - ((offset + s.seconds) / max) * plotH}
          <rect x={x0 + w * gap * 0.5} y={y} width={w * (1 - gap)} height={h} fill={s.color} rx={bars.length > 40 ? 1 : 2}>
            <title>{b.full} · {s.label}: {formatHours(s.seconds, tf)}</title>
          </rect>
        {/each}
        {#if showTargets && b.target}
          {@const ty = plotH - (b.target / max) * plotH}
          <line x1={x0 + w * 0.08} x2={x0 + w * 0.92} y1={ty} y2={ty} stroke="var(--text-2)" stroke-width="1.5" stroke-dasharray="3 2" />
        {/if}
        {#if i % labelEvery === 0}
          <text x={x0 + w / 2} y={height - 8} text-anchor="middle" font-size="10" fill="var(--text-3)">{b.label}</text>
        {/if}
      </g>
    {/each}
  </svg>
</div>

<style>
  .wrap {
    width: 100%;
  }
  svg {
    display: block;
    width: 100%;
  }
</style>
