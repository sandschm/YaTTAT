<script lang="ts">
  import { formatHours, formatPercent } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import { t } from "../lib/i18n.svelte";

  export interface Slice {
    key: string;
    label: string;
    seconds: number;
    color: string;
  }
  let { slices, center = "" }: { slices: Slice[]; center?: string } = $props();
  const tf = $derived(store.timeFormat);
  const total = $derived(slices.reduce((a, s) => a + s.seconds, 0));
  const segments = $derived.by(() => {
    const r = 46;
    const c = 2 * Math.PI * r;
    let offset = 0;
    return slices.map((s) => {
      const frac = total ? s.seconds / total : 0;
      const seg = { ...s, frac, dash: `${frac * c} ${c - frac * c}`, offset: -offset * c };
      offset += frac;
      return seg;
    });
  });
</script>

{#if total > 0}
  <div class="donut-wrap">
    <svg viewBox="0 0 120 120" class="donut" role="img" aria-label={t("chart.distribution")}>
      {#each segments as seg (seg.key)}
        <circle cx="60" cy="60" r="46" fill="none" stroke={seg.color} stroke-width="16" stroke-dasharray={seg.dash} stroke-dashoffset={seg.offset} transform="rotate(-90 60 60)">
          <title>{seg.label}: {formatHours(seg.seconds, tf)} ({formatPercent(seg.seconds, total)})</title>
        </circle>
      {/each}
      <text x="60" y="64" text-anchor="middle" font-size="12" font-weight="600" fill="var(--text)">{center || formatHours(total, tf)}</text>
    </svg>
    <ul class="legend">
      {#each segments.slice(0, 10) as seg (seg.key)}
        <li>
          <span class="dot" style="background:{seg.color}"></span>
          <span class="ellipsis grow" title={seg.label}>{seg.label}</span>
          <span class="mono">{formatPercent(seg.seconds, total)}</span>
          <span class="mono muted">{formatHours(seg.seconds, tf)}</span>
        </li>
      {/each}
      {#if segments.length > 10}<li class="faint">{t("chart.more", { n: segments.length - 10 })}</li>{/if}
    </ul>
  </div>
{:else}
  <p class="faint">{t("chart.noTimes")}</p>
{/if}

<style>
  .donut-wrap {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .donut {
    width: 140px;
    height: 140px;
    flex-shrink: 0;
  }
  .legend {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
    flex: 1;
  }
  .legend li {
    display: flex;
    gap: 8px;
    align-items: center;
    font-size: 12.5px;
  }
  .dot {
    width: 9px;
    height: 9px;
  }
</style>
