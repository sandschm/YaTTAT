<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, openExternal, type ExportRow, type IssueStat, type Stats } from "../lib/api";
  import { formatDate, formatHours, formatPercent, formatTime } from "../lib/format";
  import Icon from "../components/Icon.svelte";

  let stats = $state<Stats | null>(null);
  let loading = $state(false);
  let search = $state("");
  let project = $state("");
  let issueType = $state("");
  type Group = "none" | "epic" | "project" | "type";
  let group = $state<Group>("none");
  type SortKey = "key" | "seconds" | "estimate" | "status";
  let sortKey = $state<SortKey>("seconds");
  let sortDesc = $state(true);
  let expanded = $state<string | null>(null);
  let detail = $state<ExportRow[]>([]);
  let detailLoading = $state(false);

  const tf = $derived(store.timeFormat);

  $effect(() => {
    void store.dataVersion;
    const { from, to } = store.range;
    const ids = store.memberIds;
    if (!ids.length) {
      stats = null;
      return;
    }
    loading = true;
    api
      .getStats(from, to, ids)
      .then((s) => (stats = s))
      .catch((e) => store.toast(String(e), "error"))
      .finally(() => (loading = false));
  });

  $effect(() => {
    void store.dataVersion;
    const key = expanded;
    const { from, to } = store.range;
    const ids = store.memberIds;
    if (!key) {
      detail = [];
      return;
    }
    detailLoading = true;
    api
      .listWorklogs(from, to, ids, key, false)
      .then((r) => (detail = r))
      .catch(() => (detail = []))
      .finally(() => (detailLoading = false));
  });

  const projects = $derived(stats?.byProject ?? []);
  const types = $derived([...new Set((stats?.byIssue ?? []).map((i) => i.issueType).filter(Boolean))].sort());

  const filtered = $derived.by((): IssueStat[] => {
    if (!stats) return [];
    const q = search.trim().toLowerCase();
    const dir = sortDesc ? -1 : 1;
    const val = (i: IssueStat): number | string => {
      switch (sortKey) {
        case "key": return i.key;
        case "seconds": return i.seconds;
        case "estimate": return i.originalEstimateS ?? -1;
        case "status": return i.status;
      }
    };
    return stats.byIssue
      .filter((i) => !project || i.projectKey === project)
      .filter((i) => !issueType || i.issueType === issueType)
      .filter((i) => !q || i.key.toLowerCase().includes(q) || i.summary.toLowerCase().includes(q) || (i.epicKey ?? "").toLowerCase().includes(q))
      .sort((a, b) => {
        const x = val(a), y = val(b);
        return (x < y ? -1 : x > y ? 1 : 0) * dir;
      });
  });

  const filteredTotal = $derived(filtered.reduce((a, i) => a + i.seconds, 0));

  interface GroupRow {
    key: string;
    label: string;
    color: string | null;
    seconds: number;
    issues: IssueStat[];
  }
  const groups = $derived.by((): GroupRow[] => {
    if (group === "none") return [{ key: "all", label: "", color: null, seconds: filteredTotal, issues: filtered }];
    const map = new Map<string, GroupRow>();
    for (const i of filtered) {
      let key: string;
      let label: string;
      let color: string | null = null;
      if (group === "epic") {
        key = i.epicKey ?? "";
        const epic = stats?.byIssue.find((x) => x.key === key);
        label = key ? `${key}${epic?.summary ? ` · ${epic.summary}` : ""}` : "Ohne Epic";
        color = i.color;
      } else if (group === "project") {
        key = i.projectKey;
        const p = projects.find((x) => x.key === key);
        label = `${key}${p?.label && p.label !== key ? ` · ${p.label}` : ""}`;
        color = p?.color ?? i.color;
      } else {
        key = i.issueType || "Unbekannt";
        label = key;
      }
      const g = map.get(key) ?? { key, label, color, seconds: 0, issues: [] };
      g.seconds += i.seconds;
      g.issues.push(i);
      map.set(key, g);
    }
    return [...map.values()].sort((a, b) => b.seconds - a.seconds);
  });

  function sortBy(key: SortKey) {
    if (sortKey === key) sortDesc = !sortDesc;
    else {
      sortKey = key;
      sortDesc = key === "seconds" || key === "estimate";
    }
  }

  async function openIssue(key: string) {
    try {
      await openExternal(await api.getIssueUrl(key));
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  function memberSegments(i: IssueStat) {
    return store.members
      .filter((m) => i.byMember[m.id])
      .map((m) => ({ id: m.id, name: m.displayName, color: m.color ?? store.memberColor(m.id), seconds: i.byMember[m.id] }))
      .sort((a, b) => b.seconds - a.seconds);
  }

  function statusClass(category: string): string {
    return category === "done" ? "success" : category === "indeterminate" ? "accent" : "";
  }
</script>

<div class="view">
  <div class="view-header">
    <h1>Tickets</h1>
    <div class="search row">
      <Icon name="search" size={15} />
      <input type="search" placeholder="Ticket, Titel oder Epic …" bind:value={search} />
    </div>
    <select bind:value={project} title="Projekt">
      <option value="">Alle Projekte</option>
      {#each projects as p (p.key)}
        <option value={p.key}>{p.key} · {p.label}</option>
      {/each}
    </select>
    <select bind:value={issueType} title="Tickettyp">
      <option value="">Alle Typen</option>
      {#each types as t (t)}
        <option value={t}>{t}</option>
      {/each}
    </select>
    <label class="group-by">
      <Icon name="layers" size={14} />
      <select bind:value={group}>
        <option value="none">Nicht gruppieren</option>
        <option value="epic">Nach Epic</option>
        <option value="project">Nach Projekt</option>
        <option value="type">Nach Tickettyp</option>
      </select>
    </label>
    {#if loading}<span class="spin faint"><Icon name="refresh" size={14} /></span>{/if}
    <span class="grow"></span>
    {#if stats}
      <span class="muted">{filtered.length} Tickets · <span class="mono">{formatHours(filteredTotal, tf)}</span></span>
    {/if}
  </div>
  <div class="view-body">
    {#if !store.memberIds.length}
      <p class="muted">Keine Mitglieder im Team. Lege sie unter Einstellungen → Team & Mitglieder an.</p>
    {:else if stats}
      <div class="card table-card">
        <table>
          <thead>
            <tr>
              <th><button type="button" onclick={() => sortBy("key")}>Ticket</button></th>
              <th>Titel</th>
              <th>Typ</th>
              <th><button type="button" onclick={() => sortBy("status")}>Status</button></th>
              <th class="num"><button type="button" onclick={() => sortBy("seconds")}>Team</button></th>
              <th class="num">Anteil</th>
              <th class="members-col">Mitglieder</th>
              <th class="num"><button type="button" onclick={() => sortBy("estimate")}>Schätzung</button></th>
              <th class="num" title="Gesamt gebuchte Zeit laut Jira (alle Personen, gesamte Laufzeit)">Jira gesamt</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {#each groups as g (g.key)}
              {#if group !== "none"}
                <tr class="group-row">
                  <td colspan="4"><span class="row"><span class="dot" style="background:{g.color ?? 'var(--text-3)'}"></span><strong>{g.label}</strong><span class="faint">{g.issues.length} Tickets</span></span></td>
                  <td class="num mono"><strong>{formatHours(g.seconds, tf)}</strong></td>
                  <td class="num mono muted">{formatPercent(g.seconds, filteredTotal)}</td>
                  <td colspan="4"></td>
                </tr>
              {/if}
              {#each g.issues as i (i.key)}
                {@const segs = memberSegments(i)}
                {@const open = expanded === i.key}
                <tr class="issue-row" class:open onclick={() => (expanded = open ? null : i.key)}>
                  <td class="key"><span class="row"><span class="dot" style="background:{i.color ?? store.projectColor(i.projectKey)}"></span><span class="mono">{i.key}</span></span></td>
                  <td class="summary"><span class="ellipsis" title={i.summary}>{i.summary}</span>{#if i.epicKey && group !== "epic"}<span class="faint small"> · {i.epicKey}</span>{/if}</td>
                  <td class="muted">{i.issueType}</td>
                  <td><span class="badge {statusClass(i.statusCategory)}">{i.status || "–"}</span></td>
                  <td class="num mono">{formatHours(i.seconds, tf)}</td>
                  <td class="num mono muted">{formatPercent(i.seconds, filteredTotal)}</td>
                  <td class="members-col">
                    <span class="stack" title={segs.map((s) => `${s.name}: ${formatHours(s.seconds, tf)}`).join("\n")}>
                      {#each segs as s (s.id)}
                        <span class="seg" style="width:{(s.seconds / i.seconds) * 100}%; background:{s.color}"></span>
                      {/each}
                    </span>
                  </td>
                  <td class="num mono muted">{i.originalEstimateS != null ? formatHours(i.originalEstimateS, tf) : "–"}</td>
                  <td class="num mono muted" title={i.timeSpentS != null ? `Team-Anteil im Zeitraum: ${formatPercent(i.seconds, i.timeSpentS)}` : ""}>{i.timeSpentS != null ? formatHours(i.timeSpentS, tf) : "–"}</td>
                  <td class="actions"><button class="btn ghost sm icon" type="button" title="In Jira öffnen" onclick={(e) => { e.stopPropagation(); openIssue(i.key); }}><Icon name="external" size={14} /></button></td>
                </tr>
                {#if open}
                  <tr class="detail-row">
                    <td colspan="10">
                      {#if detailLoading}
                        <span class="faint">Lade …</span>
                      {:else if !detail.length}
                        <span class="faint">Keine Worklogs.</span>
                      {:else}
                        <div class="detail">
                          <div class="member-list">
                            {#each segs as s (s.id)}
                              <span class="row"><span class="dot" style="background:{s.color}"></span>{s.name}<span class="mono muted">{formatHours(s.seconds, tf)}</span></span>
                            {/each}
                          </div>
                          <table class="worklogs">
                            <tbody>
                              {#each detail as r (r.worklogIds.join(","))}
                                <tr>
                                  <td class="mono muted">{formatDate(r.date, { day: "2-digit", month: "2-digit", year: "numeric" })}</td>
                                  <td class="mono muted">{formatTime(r.started)}</td>
                                  <td><span class="row"><span class="dot" style="background:{store.memberColor(r.memberId)}"></span>{r.member}</span></td>
                                  <td class="comment muted ellipsis" title={r.comment}>{r.comment}</td>
                                  <td class="num mono">{formatHours(r.seconds, tf)}</td>
                                </tr>
                              {/each}
                            </tbody>
                          </table>
                        </div>
                      {/if}
                    </td>
                  </tr>
                {/if}
              {/each}
            {/each}
            {#if !filtered.length}
              <tr><td colspan="10" class="faint">Keine Tickets im Zeitraum.</td></tr>
            {/if}
          </tbody>
        </table>
      </div>
    {:else}
      <p class="faint">Lade …</p>
    {/if}
  </div>
</div>

<style>
  .small {
    font-size: 11.5px;
  }
  .search {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 8px;
    color: var(--text-3);
    width: 260px;
  }
  .search input {
    border: none;
    background: transparent;
    flex: 1;
    min-width: 0;
    box-shadow: none !important;
    color: var(--text);
    padding: 6px 0;
  }
  .group-by {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
  .table-card {
    overflow: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  th {
    text-align: left;
    color: var(--text-2);
    font-weight: 500;
    padding: 8px 10px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
    position: sticky;
    top: 0;
    background: var(--surface);
    z-index: 1;
  }
  th button {
    border: none;
    background: transparent;
    color: inherit;
    font: inherit;
    cursor: pointer;
    padding: 0;
  }
  th button:hover {
    color: var(--accent);
  }
  td {
    padding: 7px 10px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .issue-row {
    cursor: pointer;
  }
  .issue-row:hover td,
  .issue-row.open td {
    background: var(--surface-2);
  }
  .group-row td {
    background: var(--surface-3);
  }
  .key {
    white-space: nowrap;
  }
  .summary {
    max-width: 420px;
  }
  .summary .ellipsis {
    display: inline-block;
    max-width: 380px;
    vertical-align: bottom;
  }
  .num {
    text-align: right;
    white-space: nowrap;
  }
  .members-col {
    width: 140px;
  }
  .stack {
    display: flex;
    height: 10px;
    border-radius: 5px;
    overflow: hidden;
    background: var(--surface-3);
    width: 130px;
  }
  .seg {
    display: block;
    height: 100%;
  }
  .actions {
    width: 36px;
  }
  .detail-row td {
    background: var(--surface-2);
    padding: 8px 14px 12px 32px;
  }
  .detail {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .member-list {
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    font-size: 12.5px;
  }
  .worklogs td {
    border-bottom: 1px solid color-mix(in srgb, var(--border) 60%, transparent);
    padding: 4px 8px;
  }
  .comment {
    max-width: 360px;
  }
  .dot {
    width: 8px;
    height: 8px;
  }
</style>
