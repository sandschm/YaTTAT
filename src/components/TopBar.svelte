<script lang="ts">
  // Team selector, date range and fetch status. Shared by all analysis views.
  import { presets, store, type Preset } from "../lib/store.svelte";
  import { api, type CoverageInfo } from "../lib/api";
  import { formatDateTime, formatShortDate, relativeTime } from "../lib/format";
  import { t } from "../lib/i18n.svelte";
  import Icon from "./Icon.svelte";

  let coverage = $state<CoverageInfo | null>(null);
  const fetching = $derived(store.fetchStatus.inProgress);
  const progress = $derived(store.fetchStatus.progress);

  $effect(() => {
    void store.dataVersion;
    const { from, to } = store.range;
    const ids = store.memberIds;
    if (!ids.length) {
      coverage = null;
      return;
    }
    api.getCoverage(from, to, ids).then((c) => (coverage = c)).catch(() => (coverage = null));
  });

  const missing = $derived(coverage ? coverage.totalDays - coverage.coveredDays : 0);
  const tempoWithoutToken = $derived(store.settings?.tempo.source === "tempoCloud" && !store.settings?.tempo.cloudTokenSet);
  const outcome = $derived(store.fetchStatus.lastOutcome);
  const missingNames = $derived((outcome?.fetched ? outcome.authorsWithoutWorklogs : []).map((id) => store.memberName(id)));
  // Jira found worklogs of people who are not members (typically the Tempo app) while members
  // stay without times: the times are probably only reachable through Tempo.
  const otherAuthorsHint = $derived.by(() => {
    if (!outcome || !outcome.fetched || outcome.source !== "jira" || !outcome.otherAuthors.length || !missingNames.length) return "";
    return t("top.otherAuthorsHint", { names: missingNames.join(", "), authors: outcome.otherAuthors.map((o) => `${o.name || o.id}: ${o.count}`).join(", ") });
  });
  // Tempo source: members without any worklog may simply not have booked, or the token may not
  // be allowed to see them. Shown softly next to the status.
  const tempoMissingHint = $derived(outcome?.fetched && outcome.source === "tempoCloud" && missingNames.length ? t("top.tempoMissingHint", { names: missingNames.join(", ") }) : "");
  // "As of" is the OLDEST fetch in the range: later bookings and deletions since then are not in
  // the cache until the range is refreshed.
  const oldest = $derived(coverage?.oldestFetch ?? store.fetchStatus.lastSuccess);
  const staleAfterMinutes = $derived(Math.max((store.settings?.refresh.intervalMinutes ?? 30) * 2, 60));
  const stale = $derived(!!oldest && (Date.now() - new Date(oldest).getTime()) / 60000 > staleAfterMinutes);
  const standTitle = $derived(
    coverage?.oldestFetch
      ? t("top.standTitle", {
          oldest: formatDateTime(coverage.oldestFetch),
          newest: coverage.newestFetch && coverage.newestFetch !== coverage.oldestFetch ? t("top.standNewest", { newest: formatDateTime(coverage.newestFetch) }) : "",
        })
      : "",
  );

  function selectPreset(p: Preset) {
    store.preset = p;
    store.persist();
  }
</script>

<div class="topbar no-print">
  {#if store.teams.length > 1}
    <label class="team">
      <Icon name="users" size={15} />
      <select value={store.team?.id ?? ""} onchange={(e) => { store.teamId = (e.target as HTMLSelectElement).value; store.memberId = null; store.persist(); }}>
        {#each store.teams as team (team.id)}
          <option value={team.id}>{team.name}</option>
        {/each}
      </select>
    </label>
  {:else if store.team}
    <span class="team static"><Icon name="users" size={15} /> {store.team.name}</span>
  {/if}
  <div class="seg">
    {#each presets as p (p.id)}
      <button class:on={store.preset === p.id} type="button" onclick={() => selectPreset(p.id)}>{t(p.label)}</button>
    {/each}
  </div>
  {#if store.preset === "custom"}
    <input type="date" bind:value={store.customFrom} onchange={() => store.persist()} />
    <span class="faint">{t("common.to")}</span>
    <input type="date" bind:value={store.customTo} onchange={() => store.persist()} />
  {:else}
    <span class="muted range">{formatShortDate(store.range.from)} – {formatShortDate(store.range.to)}</span>
  {/if}
  <span class="grow"></span>
  <div class="status">
    {#if fetching}
      <span class="spin"><Icon name="refresh" size={14} /></span>
      {#if progress?.phase === "worklogs" && progress.total}
        <span>{t("top.worklogsProgress", { done: progress.done, total: progress.total })}</span>
        <span class="bar"><span class="fill" style="width:{(progress.done / progress.total) * 100}%"></span></span>
      {:else}
        <span>{t("top.searchingIssues")}</span>
      {/if}
    {:else if !store.connected}
      <span class="warn"><Icon name="warning" size={13} /> {t("top.notConnected")}</span>
    {:else if !store.memberIds.length}
      <span class="warn"><Icon name="warning" size={13} /> {t("top.noMembers")}</span>
    {:else if tempoWithoutToken}
      <button type="button" class="warn link" onclick={() => (store.view = "settings")} title={t("top.tempoTokenMissingTitle")}><Icon name="warning" size={13} /> {t("top.tempoTokenMissing")}</button>
    {:else if missing > 0}
      <span class="warn" title={t("top.daysMissingTitle", { n: missing, total: coverage?.totalDays ?? 0 })}><Icon name="warning" size={13} /> {t("top.daysMissing", { n: missing })}</span>
    {:else if store.fetchStatus.lastError}
      <span class="warn" title={store.fetchStatus.lastError}><Icon name="warning" size={13} /> {t("top.fetchFailed")}</span>
    {:else if otherAuthorsHint}
      <button type="button" class="warn link" onclick={() => (store.view = "settings")} title="{otherAuthorsHint} {t('top.otherAuthorsAdvice')}"><Icon name="warning" size={13} /> {t("top.otherAuthors")}</button>
    {:else if stale}
      <button type="button" class="warn link" onclick={() => store.ensureData(true)} title={standTitle}><Icon name="warning" size={13} /> {t("top.standStale", { when: relativeTime(oldest) })}</button>
    {:else}
      <span class="faint" title={standTitle}>{t("top.stand", { when: relativeTime(oldest) })}</span>
    {/if}
    {#if tempoMissingHint}
      <span class="faint hint-dot" title={tempoMissingHint}><Icon name="info" size={13} /> {t("top.withoutTimes", { n: missingNames.length })}</span>
    {/if}
    <button class="btn sm" type="button" onclick={() => store.ensureData(true)} disabled={fetching || !store.connected || !store.memberIds.length} title={t("top.refreshTitle")}>
      <Icon name="refresh" size={14} /> {t("top.refresh")}
    </button>
  </div>
</div>

<style>
  .topbar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
    flex-wrap: wrap;
  }
  .team {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
    font-weight: 500;
  }
  .team select {
    padding: 4px 8px;
    font-weight: 500;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }
  .seg button {
    border: none;
    background: var(--surface);
    padding: 5px 9px;
    cursor: pointer;
    color: var(--text-2);
    font-size: 12.5px;
    white-space: nowrap;
  }
  .seg button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .range {
    font-size: 12.5px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .warn {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--warning);
  }
  .warn.link {
    border: none;
    background: transparent;
    cursor: pointer;
    font: inherit;
    padding: 0;
  }
  .warn.link:hover {
    text-decoration: underline;
  }
  .hint-dot {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    cursor: help;
  }
  .bar {
    width: 90px;
    height: 6px;
    background: var(--surface-3);
    border-radius: 3px;
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.15s;
  }
  input[type="date"] {
    padding: 4px 6px;
    font-size: 12.5px;
  }
</style>
