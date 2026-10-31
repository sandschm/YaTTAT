<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type Stats } from "../lib/api";
  import { formatDate, formatHours, formatSigned, isoWeek, parseDate, relativeTime, startOfWeek, toDateKey } from "../lib/format";
  import BarChart, { type Bar } from "../components/BarChart.svelte";
  import HBars, { type Row } from "../components/HBars.svelte";
  import Donut from "../components/Donut.svelte";
  import Avatar from "../components/Avatar.svelte";
  import Icon from "../components/Icon.svelte";

  let stats = $state<Stats | null>(null);
  let loading = $state(false);
  type SortKey = "name" | "seconds" | "target" | "balance" | "issues" | "days" | "avg" | "last";
  let sortKey = $state<SortKey>("seconds");
  let sortDesc = $state(true);

  const tf = $derived(store.timeFormat);
  const showTargets = $derived(store.showTargets);

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

  const balance = $derived(stats ? stats.totalSeconds - stats.targetSeconds : 0);
  const activeMembers = $derived(stats ? stats.members.filter((m) => m.seconds > 0).length : 0);

  const memberRows = $derived.by((): Row[] =>
    (stats?.members ?? []).map((m) => ({
      key: m.id,
      label: m.displayName,
      sub: `${m.issueCount} Tickets · ${m.daysLogged} Tage`,
      seconds: m.seconds,
      target: m.targetSeconds,
      color: m.color ?? store.memberColor(m.id),
      avatar: { name: m.displayName, url: store.memberById(m.id)?.avatarUrl ?? null },
    })),
  );

  // Daily bars for short ranges, weekly bars for long ones, stacked by member.
  const bars = $derived.by((): Bar[] => {
    if (!stats) return [];
    const members = stats.members;
    const segmentsFor = (byMember: Record<string, number>) =>
      members.filter((m) => byMember[m.id]).map((m) => ({ key: m.id, label: m.displayName, color: m.color ?? store.memberColor(m.id), seconds: byMember[m.id] }));
    if (stats.days.length <= 31) {
      return stats.days.map((d) => ({
        label: stats!.days.length <= 7 ? formatDate(d.date, { weekday: "short" }) : String(parseDate(d.date).getDate()),
        full: formatDate(d.date),
        segments: segmentsFor(d.byMember),
        target: d.targetSeconds,
        muted: (parseDate(d.date).getDay() + 6) % 7 >= 5 && d.seconds === 0,
      }));
    }
    const weeks = new Map<string, { label: string; full: string; byMember: Record<string, number>; target: number }>();
    for (const d of stats.days) {
      const ws = startOfWeek(parseDate(d.date), store.firstDay);
      const k = toDateKey(ws);
      const w = weeks.get(k) ?? { label: `KW ${isoWeek(ws)}`, full: `Woche ab ${formatDate(ws, { day: "2-digit", month: "2-digit" })}`, byMember: {}, target: 0 };
      for (const [id, s] of Object.entries(d.byMember)) w.byMember[id] = (w.byMember[id] ?? 0) + s;
      w.target += d.targetSeconds;
      weeks.set(k, w);
    }
    return [...weeks.values()].map((w) => ({ label: w.label, full: w.full, segments: segmentsFor(w.byMember), target: w.target }));
  });

  const weekdayBars = $derived.by((): Bar[] => {
    if (!stats) return [];
    const names = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
    return stats.byWeekday.map((s, i) => ({ label: names[i], full: names[i], segments: [{ key: "all", label: "Team", color: "var(--accent)", seconds: s }] }));
  });

  const typeRows = $derived.by((): Row[] => {
    const palette = ["#2f6fed", "#17956b", "#e8572a", "#8b5cf6", "#ec4899", "#0ea5e9", "#f59e0b"];
    return (stats?.byIssueType ?? []).map((g, i) => ({ key: g.key, label: g.label, seconds: g.seconds, color: palette[i % palette.length] }));
  });

  const sortedMembers = $derived.by(() => {
    if (!stats) return [];
    const dir = sortDesc ? -1 : 1;
    const val = (m: Stats["members"][number]): number | string => {
      switch (sortKey) {
        case "name": return m.displayName.toLowerCase();
        case "seconds": return m.seconds;
        case "target": return m.targetSeconds;
        case "balance": return m.seconds - m.targetSeconds;
        case "issues": return m.issueCount;
        case "days": return m.daysLogged;
        case "avg": return m.avgPerLoggedDaySeconds;
        case "last": return m.lastWorklog ?? "";
      }
    };
    return [...stats.members].sort((a, b) => {
      const x = val(a), y = val(b);
      return (x < y ? -1 : x > y ? 1 : 0) * dir;
    });
  });

  function sortBy(key: SortKey) {
    if (sortKey === key) sortDesc = !sortDesc;
    else {
      sortKey = key;
      sortDesc = key !== "name";
    }
  }

  function openMember(id: string) {
    store.memberId = id;
    store.persist();
    store.view = "member";
  }
</script>

<div class="view">
  <div class="view-header">
    <h1>Team{store.team ? `: ${store.team.name}` : ""}</h1>
    {#if loading}<span class="spin faint"><Icon name="refresh" size={14} /></span>{/if}
  </div>
  <div class="view-body">
    {#if !store.memberIds.length}
      <div class="card empty">
        <Icon name="users" size={28} />
        <h3>Noch keine Teammitglieder</h3>
        <p class="muted">Lege unter Einstellungen → Team & Mitglieder fest, wessen Zeiten ausgewertet werden sollen.</p>
        <button class="btn primary" type="button" onclick={() => (store.view = "settings")}>Zu den Einstellungen</button>
      </div>
    {:else if stats}
      <div class="kpis">
        <div class="kpi card">
          <span class="label">Erfasst</span>
          <span class="value mono">{formatHours(stats.totalSeconds, tf)}</span>
          <span class="sub faint">{stats.worklogCount} Worklogs an {stats.daysLogged} Tagen</span>
        </div>
        {#if showTargets}
          <div class="kpi card">
            <span class="label">Soll</span>
            <span class="value mono">{formatHours(stats.targetSeconds, tf)}</span>
            <span class="sub faint">{stats.workDays} Arbeitstage bis heute</span>
          </div>
          <div class="kpi card" class:pos={balance >= 0} class:neg={balance < 0}>
            <span class="label">Saldo</span>
            <span class="value mono">{formatSigned(balance, tf)}</span>
            <span class="sub faint">{stats.targetSeconds ? Math.round((stats.totalSeconds / stats.targetSeconds) * 100) : 0} % des Solls</span>
          </div>
        {/if}
        <div class="kpi card">
          <span class="label">Ø pro Mitglied</span>
          <span class="value mono">{formatHours(activeMembers ? stats.totalSeconds / activeMembers : 0, tf)}</span>
          <span class="sub faint">{activeMembers} von {stats.members.length} mit Zeiten</span>
        </div>
        <div class="kpi card">
          <span class="label">Tickets</span>
          <span class="value mono">{stats.issueCount}</span>
          <span class="sub faint">{stats.byProject.length} {stats.byProject.length === 1 ? "Projekt" : "Projekte"}</span>
        </div>
        <div class="kpi card">
          <span class="label">Ø pro Tag</span>
          <span class="value mono">{formatHours(stats.daysLogged ? stats.totalSeconds / stats.daysLogged : 0, tf)}</span>
          <span class="sub faint">an Tagen mit Buchungen</span>
        </div>
      </div>

      <div class="grid">
        <section class="card panel">
          <h3>Stunden pro Mitglied</h3>
          <HBars rows={memberRows} {showTargets} onselect={(r) => openMember(r.key)} />
        </section>
        <section class="card panel">
          <h3>Verteilung nach Projekt</h3>
          <Donut slices={stats.byProject.map((p) => ({ key: p.key, label: `${p.key} · ${p.label}`, seconds: p.seconds, color: p.color ?? store.projectColor(p.key) }))} />
        </section>
        <section class="card panel wide">
          <h3>Stunden pro {bars.length === stats.days.length ? "Tag" : "Woche"} nach Mitglied</h3>
          <BarChart {bars} {showTargets} />
          <div class="legend">
            {#each stats.members as m (m.id)}
              <span class="legend-item"><span class="dot" style="background:{m.color ?? store.memberColor(m.id)}"></span>{m.displayName}</span>
            {/each}
            {#if showTargets}<span class="legend-item faint"><span class="dash"></span>Soll</span>{/if}
          </div>
        </section>
        <section class="card panel">
          <h3>Nach Tickettyp</h3>
          <HBars rows={typeRows} labelWidth={120} />
        </section>
        <section class="card panel">
          <h3>Nach Wochentag</h3>
          <BarChart bars={weekdayBars} height={150} showTargets={false} />
        </section>
        <section class="card panel wide table-panel">
          <h3>Mitglieder</h3>
          <table>
            <thead>
              <tr>
                <th><button type="button" onclick={() => sortBy("name")}>Mitglied</button></th>
                <th class="num"><button type="button" onclick={() => sortBy("seconds")}>Erfasst</button></th>
                {#if showTargets}
                  <th class="num"><button type="button" onclick={() => sortBy("target")}>Soll</button></th>
                  <th class="num"><button type="button" onclick={() => sortBy("balance")}>Saldo</button></th>
                {/if}
                <th class="num"><button type="button" onclick={() => sortBy("issues")}>Tickets</button></th>
                <th class="num"><button type="button" onclick={() => sortBy("days")}>Tage</button></th>
                <th class="num"><button type="button" onclick={() => sortBy("avg")}>Ø/Tag</button></th>
                <th><button type="button" onclick={() => sortBy("last")}>Letzter Worklog</button></th>
                <th>Projekte</th>
              </tr>
            </thead>
            <tbody>
              {#each sortedMembers as m (m.id)}
                {@const b = m.seconds - m.targetSeconds}
                <tr onclick={() => openMember(m.id)}>
                  <td><span class="row"><Avatar name={m.displayName} url={store.memberById(m.id)?.avatarUrl} color={m.color} size={24} /><span>{m.displayName}</span></span></td>
                  <td class="num mono">{formatHours(m.seconds, tf)}</td>
                  {#if showTargets}
                    <td class="num mono muted">{formatHours(m.targetSeconds, tf)}</td>
                    <td class="num mono" class:pos={b >= 0} class:neg={b < 0}>{formatSigned(b, tf)}</td>
                  {/if}
                  <td class="num">{m.issueCount}</td>
                  <td class="num">{m.daysLogged}</td>
                  <td class="num mono">{formatHours(m.avgPerLoggedDaySeconds, tf)}</td>
                  <td class="muted">{relativeTime(m.lastWorklog)}</td>
                  <td>
                    <span class="chips">
                      {#each m.byProject.slice(0, 4) as p (p.key)}
                        <span class="badge" style="background: color-mix(in srgb, {p.color ?? store.projectColor(p.key)} 18%, transparent); color: {p.color ?? store.projectColor(p.key)}">{p.key} {Math.round((p.seconds / Math.max(1, m.seconds)) * 100)} %</span>
                      {/each}
                    </span>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </section>
      </div>
    {:else}
      <p class="faint">Lade …</p>
    {/if}
  </div>
</div>

<style>
  .kpis {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(150px, 1fr));
    gap: 10px;
    margin-bottom: 14px;
  }
  .kpi {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .kpi .label {
    font-size: 12px;
    color: var(--text-2);
  }
  .kpi .value {
    font-size: 20px;
    font-weight: 600;
  }
  .kpi.pos .value,
  td.pos {
    color: var(--success);
  }
  .kpi.neg .value,
  td.neg {
    color: var(--danger);
  }
  .kpi .sub {
    font-size: 11.5px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .panel {
    padding: 14px 16px;
    min-width: 0;
  }
  .panel.wide {
    grid-column: 1 / -1;
  }
  .panel h3 {
    margin: 0 0 10px;
    font-size: 13.5px;
    color: var(--text-2);
    font-weight: 600;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-2);
  }
  .legend-item {
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .legend .dot {
    width: 9px;
    height: 9px;
  }
  .dash {
    width: 14px;
    border-top: 2px dashed var(--text-2);
  }
  .table-panel {
    overflow-x: auto;
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
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
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
    padding: 7px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  tbody tr {
    cursor: pointer;
  }
  tbody tr:hover td {
    background: var(--surface-2);
  }
  .num {
    text-align: right;
  }
  .chips {
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
  }
  .empty {
    padding: 40px;
    text-align: center;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
  }
  .empty h3 {
    margin: 0;
    color: var(--text);
  }
  @media (max-width: 1000px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
