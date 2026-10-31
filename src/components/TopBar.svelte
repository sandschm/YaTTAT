<script lang="ts">
  // Team selector, date range and fetch status. Shared by all analysis views.
  import { presets, store, type Preset } from "../lib/store.svelte";
  import { api, type CoverageInfo } from "../lib/api";
  import { formatDate, relativeTime } from "../lib/format";
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
  // Jira found worklogs of people who are not members (typically the Tempo app) but none or few
  // for the members themselves: the times are probably only reachable through Tempo.
  const outcome = $derived(store.fetchStatus.lastOutcome);
  const otherAuthorsHint = $derived.by(() => {
    if (!outcome || !outcome.fetched || outcome.source !== "jira" || !outcome.otherAuthors.length) return "";
    if (outcome.authorsWithWorklogs >= outcome.authors) return "";
    return `${outcome.authors - outcome.authorsWithWorklogs} von ${outcome.authors} Mitgliedern ohne Zeiten; Jira führt im Zeitraum Worklogs unter anderen Autoren: ${outcome.otherAuthors.map((o) => `${o.name || o.id}: ${o.count}`).join(", ")}`;
  });

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
        {#each store.teams as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </label>
  {:else if store.team}
    <span class="team static"><Icon name="users" size={15} /> {store.team.name}</span>
  {/if}
  <div class="seg">
    {#each presets as p (p.id)}
      <button class:on={store.preset === p.id} type="button" onclick={() => selectPreset(p.id)}>{p.label}</button>
    {/each}
  </div>
  {#if store.preset === "custom"}
    <input type="date" bind:value={store.customFrom} onchange={() => store.persist()} />
    <span class="faint">bis</span>
    <input type="date" bind:value={store.customTo} onchange={() => store.persist()} />
  {:else}
    <span class="muted range">{formatDate(store.range.from, { day: "2-digit", month: "2-digit", year: "numeric" })} – {formatDate(store.range.to, { day: "2-digit", month: "2-digit", year: "numeric" })}</span>
  {/if}
  <span class="grow"></span>
  <div class="status">
    {#if fetching}
      <span class="spin"><Icon name="refresh" size={14} /></span>
      {#if progress?.phase === "worklogs" && progress.total}
        <span>Worklogs {progress.done}/{progress.total}</span>
        <span class="bar"><span class="fill" style="width:{(progress.done / progress.total) * 100}%"></span></span>
      {:else}
        <span>Tickets suchen …</span>
      {/if}
    {:else if !store.connected}
      <span class="warn"><Icon name="warning" size={13} /> Nicht verbunden</span>
    {:else if !store.memberIds.length}
      <span class="warn"><Icon name="warning" size={13} /> Keine Mitglieder im Team</span>
    {:else if tempoWithoutToken}
      <button type="button" class="warn link" onclick={() => (store.view = "settings")} title="Einstellungen → Verbindung → Quelle der Zeiten"><Icon name="warning" size={13} /> Tempo-API-Token fehlt</button>
    {:else if missing > 0}
      <span class="warn" title="{missing} von {coverage?.totalDays} Tagen sind noch nicht aus Jira geladen"><Icon name="warning" size={13} /> {missing} Tage fehlen</span>
    {:else if store.fetchStatus.lastError}
      <span class="warn" title={store.fetchStatus.lastError}><Icon name="warning" size={13} /> Abruf fehlgeschlagen</span>
    {:else if otherAuthorsHint}
      <button type="button" class="warn link" onclick={() => (store.view = "settings")} title="{otherAuthorsHint}. Werden die Zeiten in Tempo gebucht, unter Einstellungen → Verbindung die Quelle „Tempo Cloud“ wählen."><Icon name="warning" size={13} /> Worklogs anderer Autoren gefunden</button>
    {:else}
      <span class="faint" title={coverage?.newestFetch ? `Stand ${formatDate(coverage.newestFetch, { day: "2-digit", month: "2-digit", year: "numeric", hour: "2-digit", minute: "2-digit" })}` : ""}>Stand: {relativeTime(coverage?.newestFetch ?? store.fetchStatus.lastSuccess)}</span>
    {/if}
    <button class="btn sm" type="button" onclick={() => store.ensureData(true)} disabled={fetching || !store.connected || !store.memberIds.length} title="Zeitraum neu aus Jira laden">
      <Icon name="refresh" size={14} /> Aktualisieren
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
