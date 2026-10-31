<script lang="ts">
  // Horizontal bars (one row per item) with an optional target marker. Used for members, tickets, types.
  import { formatHours, formatSigned } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import Avatar from "./Avatar.svelte";
  import { t } from "../lib/i18n.svelte";

  export interface Row {
    key: string;
    label: string;
    sub?: string;
    seconds: number;
    target?: number;
    color: string;
    avatar?: { name: string; url: string | null } | null;
  }

  let { rows, showTargets = false, onselect, labelWidth = 160 }: { rows: Row[]; showTargets?: boolean; onselect?: (row: Row) => void; labelWidth?: number } = $props();
  const tf = $derived(store.timeFormat);
  const max = $derived(Math.max(1, ...rows.map((r) => Math.max(r.seconds, showTargets ? (r.target ?? 0) : 0))));
</script>

<ul class="hbars">
  {#each rows as r (r.key)}
    <li>
      <button type="button" class="row-btn" class:clickable={!!onselect} onclick={() => onselect?.(r)} disabled={!onselect}>
        <span class="label" style="width:{labelWidth}px">
          {#if r.avatar}<Avatar name={r.avatar.name} url={r.avatar.url} color={r.color} size={22} />{:else}<span class="dot" style="background:{r.color}"></span>{/if}
          <span class="texts">
            <span class="ellipsis">{r.label}</span>
            {#if r.sub}<span class="sub ellipsis">{r.sub}</span>{/if}
          </span>
        </span>
        <span class="track">
          <span class="fill" style="width:{(r.seconds / max) * 100}%; background:{r.color}"></span>
          {#if showTargets && r.target}
            <span class="target" style="left:{(r.target / max) * 100}%" title={t("chart.target") + " " + formatHours(r.target, tf)}></span>
          {/if}
        </span>
        <span class="value mono">{formatHours(r.seconds, tf)}</span>
        {#if showTargets && r.target}
          <span class="delta mono" class:pos={r.seconds >= r.target} class:neg={r.seconds < r.target}>{formatSigned(r.seconds - r.target, tf)}</span>
        {/if}
      </button>
    </li>
  {/each}
  {#if !rows.length}<li class="faint">{t("chart.noTimes")}</li>{/if}
</ul>

<style>
  .hbars {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .row-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    border: none;
    background: transparent;
    padding: 3px 4px;
    border-radius: 6px;
    text-align: left;
    color: inherit;
    font-size: 12.5px;
  }
  .row-btn.clickable {
    cursor: pointer;
  }
  .row-btn.clickable:hover {
    background: var(--surface-2);
  }
  .label {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
    min-width: 0;
  }
  .texts {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.25;
  }
  .sub {
    color: var(--text-3);
    font-size: 11.5px;
  }
  .dot {
    width: 9px;
    height: 9px;
  }
  .track {
    flex: 1;
    position: relative;
    height: 10px;
    background: var(--surface-3);
    border-radius: 5px;
    overflow: visible;
  }
  .fill {
    display: block;
    height: 100%;
    border-radius: 5px;
    transition: width 0.25s;
  }
  .target {
    position: absolute;
    top: -3px;
    width: 2px;
    height: 16px;
    background: var(--text-2);
    border-radius: 1px;
  }
  .value {
    width: 64px;
    text-align: right;
    flex-shrink: 0;
  }
  .delta {
    width: 62px;
    text-align: right;
    flex-shrink: 0;
    font-size: 11.5px;
  }
  .delta.pos {
    color: var(--success);
  }
  .delta.neg {
    color: var(--danger);
  }
</style>
