<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, isTauri, openExternal, type ExportRow, type Stats } from "../lib/api";
  import { formatDate, formatDuration, formatHours, formatSigned, formatTime, isoWeek, parseDate, startOfWeek, toDateKey } from "../lib/format";
  import BarChart, { type Bar } from "../components/BarChart.svelte";
  import HBars, { type Row } from "../components/HBars.svelte";
  import Donut from "../components/Donut.svelte";
  import Avatar from "../components/Avatar.svelte";
  import Icon from "../components/Icon.svelte";

  let stats = $state<Stats | null>(null);
  let rows = $state<ExportRow[]>([]);
  let loading = $state(false);
  let exporting = $state(false);
  let summarize = $state(false);
  let openDays = $state<Record<string, boolean>>({});

  const tf = $derived(store.timeFormat);
  const showTargets = $derived(store.showTargets);
  const member = $derived(store.member);

  $effect(() => {
    void store.dataVersion;
    const { from, to } = store.range;
    const id = member?.id;
    const sum = summarize;
    if (!id) {
      stats = null;
      rows = [];
      return;
    }
    loading = true;
    Promise.all([api.getStats(from, to, [id]), api.listWorklogs(from, to, [id], null, sum)])
      .then(([s, r]) => {
        stats = s;
        rows = r;
      })
      .catch((e) => store.toast(String(e), "error"))
      .finally(() => (loading = false));
  });

  const me = $derived(stats?.members[0] ?? null);
  const balance = $derived(me ? me.seconds - me.targetSeconds : 0);

  const bars = $derived.by((): Bar[] => {
    if (!stats || !member) return [];
    const color = member.color ?? store.memberColor(member.id);
    if (stats.days.length <= 31) {
      return stats.days.map((d) => ({
        label: stats!.days.length <= 7 ? formatDate(d.date, { weekday: "short" }) : String(parseDate(d.date).getDate()),
        full: formatDate(d.date),
        segments: [{ key: "m", label: member!.displayName, color, seconds: d.seconds }],
        target: d.targetSeconds,
      }));
    }
    const weeks = new Map<string, Bar>();
    for (const d of stats.days) {
      const ws = startOfWeek(parseDate(d.date), store.firstDay);
      const k = toDateKey(ws);
      const w = weeks.get(k) ?? { label: `KW ${isoWeek(ws)}`, full: `Woche ab ${formatDate(ws, { day: "2-digit", month: "2-digit" })}`, segments: [{ key: "m", label: member!.displayName, color, seconds: 0 }], target: 0 };
      w.segments[0].seconds += d.seconds;
      w.target = (w.target ?? 0) + d.targetSeconds;
      weeks.set(k, w);
    }
    return [...weeks.values()];
  });

  const issueRows = $derived.by((): Row[] =>
    (stats?.byIssue ?? []).slice(0, 15).map((i) => ({
      key: i.key,
      label: i.key,
      sub: i.summary,
      seconds: i.seconds,
      color: i.color ?? store.projectColor(i.projectKey),
    })),
  );

  // Worklog table grouped by day (newest first).
  const days = $derived.by(() => {
    const map = new Map<string, { date: string; seconds: number; rows: ExportRow[] }>();
    for (const r of rows) {
      const d = map.get(r.date) ?? { date: r.date, seconds: 0, rows: [] };
      d.seconds += r.seconds;
      d.rows.push(r);
      map.set(r.date, d);
    }
    return [...map.values()].sort((a, b) => b.date.localeCompare(a.date));
  });

  async function openIssue(key: string) {
    try {
      await openExternal(await api.getIssueUrl(key));
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  async function exportFile(format: "csv" | "json") {
    if (!member) return;
    exporting = true;
    try {
      const name = `yattat-${member.displayName.replace(/\s+/g, "_")}-${store.range.from}_${store.range.to}.${format}`;
      if (isTauri) {
        const { save } = await import("@tauri-apps/plugin-dialog");
        const path = await save({ defaultPath: name, filters: [{ name: format.toUpperCase(), extensions: [format] }] });
        if (!path) return;
        const n = await api.exportWorklogs(store.range.from, store.range.to, [member.id], format, summarize, path);
        store.toast(`${n} Zeilen exportiert`, "success");
      } else {
        const content = format === "json" ? JSON.stringify(rows, null, 2) : rowsToCsv(rows);
        const blob = new Blob([content], { type: format === "json" ? "application/json" : "text/csv;charset=utf-8" });
        const a = document.createElement("a");
        a.href = URL.createObjectURL(blob);
        a.download = name;
        a.click();
        URL.revokeObjectURL(a.href);
      }
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      exporting = false;
    }
  }

  function rowsToCsv(list: ExportRow[]): string {
    const head = ["Datum", "Beginn", "Mitglied", "Projekt", "Ticket", "Titel", "Typ", "Dauer", "Dauer (Sekunden)", "Kommentar", "Worklog-IDs"];
    const esc = (v: string) => (/[;"\n]/.test(v) ? `"${v.replace(/"/g, '""')}"` : v);
    const lines = list.map((r) =>
      [formatDate(r.date, { day: "2-digit", month: "2-digit", year: "numeric" }), formatTime(r.started), r.member, r.projectKey, r.issueKey, r.summary, r.issueType, formatDuration(r.seconds, tf), String(r.seconds), r.comment.replace(/\r?\n/g, " "), r.worklogIds.join(",")]
        .map(esc)
        .join(";"),
    );
    return "﻿" + [head.join(";"), ...lines].join("\n");
  }
</script>

<div class="view">
  <div class="view-header">
    <h1>Mitglied</h1>
    <div class="members">
      {#each store.members as m (m.id)}
        <button type="button" class="member-chip" class:on={member?.id === m.id} onclick={() => { store.memberId = m.id; store.persist(); }} style="--c:{m.color ?? store.memberColor(m.id)}">
          <Avatar name={m.displayName} url={m.avatarUrl} color={m.color} size={22} />
          <span>{m.displayName}</span>
        </button>
      {/each}
    </div>
    {#if loading}<span class="spin faint"><Icon name="refresh" size={14} /></span>{/if}
  </div>
  <div class="view-body">
    {#if !member}
      <p class="muted">Keine Mitglieder im Team. Lege sie unter Einstellungen → Team & Mitglieder an.</p>
    {:else if stats && me}
      <div class="kpis">
        <div class="kpi card">
          <span class="label">Erfasst</span>
          <span class="value mono">{formatHours(me.seconds, tf)}</span>
          <span class="sub faint">{me.worklogCount} Worklogs an {me.daysLogged} Tagen</span>
        </div>
        {#if showTargets}
          <div class="kpi card">
            <span class="label">Soll</span>
            <span class="value mono">{formatHours(me.targetSeconds, tf)}</span>
            <span class="sub faint">{member.weeklyHours != null ? `${member.weeklyHours} h/Woche` : "Vollzeit-Muster"}</span>
          </div>
          <div class="kpi card" class:pos={balance >= 0} class:neg={balance < 0}>
            <span class="label">Saldo</span>
            <span class="value mono">{formatSigned(balance, tf)}</span>
            <span class="sub faint">{balance >= 0 ? "über Soll" : "unter Soll"}</span>
          </div>
        {/if}
        <div class="kpi card">
          <span class="label">Ø pro Tag</span>
          <span class="value mono">{formatHours(me.avgPerLoggedDaySeconds, tf)}</span>
          <span class="sub faint">an Tagen mit Buchungen</span>
        </div>
        <div class="kpi card">
          <span class="label">Tickets</span>
          <span class="value mono">{me.issueCount}</span>
          <span class="sub faint">{me.byProject.length} {me.byProject.length === 1 ? "Projekt" : "Projekte"}</span>
        </div>
      </div>

      <div class="grid">
        <section class="card panel wide">
          <h3>Stunden pro {bars.length === stats.days.length ? "Tag" : "Woche"}</h3>
          <BarChart {bars} {showTargets} />
        </section>
        <section class="card panel">
          <h3>Verteilung nach Projekt</h3>
          <Donut slices={me.byProject.map((p) => ({ key: p.key, label: `${p.key} · ${p.label}`, seconds: p.seconds, color: p.color ?? store.projectColor(p.key) }))} />
        </section>
        <section class="card panel">
          <h3>Top-Tickets</h3>
          <HBars rows={issueRows} labelWidth={220} onselect={(r) => openIssue(r.key)} />
        </section>
        <section class="card panel wide">
          <div class="row head">
            <h3>Worklogs</h3>
            <span class="grow"></span>
            <label class="check" style="margin:0"><input type="checkbox" bind:checked={summarize} /><span>Pro Tag und Ticket zusammenfassen</span></label>
            <button class="btn sm" type="button" onclick={() => exportFile("csv")} disabled={exporting || !rows.length}><Icon name="download" size={14} /> CSV</button>
            <button class="btn sm" type="button" onclick={() => exportFile("json")} disabled={exporting || !rows.length}><Icon name="download" size={14} /> JSON</button>
          </div>
          {#if !days.length}
            <p class="faint">Keine Worklogs im Zeitraum.</p>
          {/if}
          {#each days as d (d.date)}
            {@const open = openDays[d.date] ?? days.length <= 12}
            <div class="day">
              <button type="button" class="day-head" onclick={() => (openDays = { ...openDays, [d.date]: !open })}>
                <span class="chev" class:open><Icon name="chevronRight" size={14} /></span>
                <span class="day-date">{formatDate(d.date)}</span>
                <span class="faint">{d.rows.length} {d.rows.length === 1 ? "Eintrag" : "Einträge"}</span>
                <span class="grow"></span>
                <span class="mono">{formatHours(d.seconds, tf)}</span>
              </button>
              {#if open}
                <table>
                  <tbody>
                    {#each d.rows as r (r.worklogIds.join(","))}
                      <tr>
                        <td class="time mono muted">{formatTime(r.started)}</td>
                        <td class="key"><button type="button" class="link mono" onclick={() => openIssue(r.issueKey)}>{r.issueKey}</button></td>
                        <td class="summary"><span class="dot" style="background:{store.projectColor(r.projectKey)}"></span><span class="ellipsis">{r.summary}</span></td>
                        <td class="comment muted ellipsis" title={r.comment}>{r.comment}</td>
                        <td class="num mono">{formatHours(r.seconds, tf)}</td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {/if}
            </div>
          {/each}
        </section>
      </div>
    {:else}
      <p class="faint">Lade …</p>
    {/if}
  </div>
</div>

<style>
  .members {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .member-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 10px 3px 4px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface);
    cursor: pointer;
    color: var(--text-2);
    font-size: 12.5px;
  }
  .member-chip.on {
    border-color: var(--c);
    background: color-mix(in srgb, var(--c) 14%, var(--surface));
    color: var(--text);
    font-weight: 600;
  }
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
  .kpi.pos .value {
    color: var(--success);
  }
  .kpi.neg .value {
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
  .head {
    margin-bottom: 6px;
  }
  .head h3 {
    margin: 0;
  }
  .day {
    border-top: 1px solid var(--border);
  }
  .day-head {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: transparent;
    padding: 8px 4px;
    cursor: pointer;
    color: inherit;
    text-align: left;
  }
  .day-head:hover {
    background: var(--surface-2);
  }
  .chev {
    display: inline-flex;
    transition: transform 0.15s;
    color: var(--text-3);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .day-date {
    font-weight: 600;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
    margin-bottom: 6px;
  }
  td {
    padding: 4px 8px;
    vertical-align: middle;
  }
  .time {
    width: 50px;
  }
  .key {
    width: 90px;
  }
  .summary {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    max-width: 420px;
  }
  .summary .dot {
    width: 8px;
    height: 8px;
  }
  .comment {
    max-width: 300px;
  }
  .num {
    text-align: right;
    width: 70px;
  }
  .link {
    border: none;
    background: transparent;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
    font-size: inherit;
  }
  .link:hover {
    text-decoration: underline;
  }
  @media (max-width: 1000px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }
</style>
