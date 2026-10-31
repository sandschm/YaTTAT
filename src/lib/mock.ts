// In-browser mock of the Tauri backend. Used when the app runs outside Tauri (`npm run dev` opened
// in a normal browser) so that the UI can be developed and reviewed without Jira. Sample data:
// four team members, three projects, deterministic worklogs for the last 200 days.
import type {
  AppInfo,
  AuthStatus,
  CacheInfo,
  CoverageInfo,
  DayStat,
  ExportRow,
  FetchStatus,
  GroupStat,
  IssueStat,
  JiraUser,
  Member,
  MemberStat,
  Project,
  Settings,
  Stats,
} from "./api";

type Handler = (payload: unknown) => void;
const handlers = new Map<string, Set<Handler>>();

export function mockEmit(event: string, payload: unknown = null) {
  handlers.get(event)?.forEach((h) => h(payload));
}

export function mockListen(event: string, handler: Handler): () => void {
  if (!handlers.has(event)) handlers.set(event, new Set());
  handlers.get(event)!.add(handler);
  return () => handlers.get(event)?.delete(handler);
}

const users: JiraUser[] = [
  { id: "me", key: "JIRAUSER1", displayName: "Sam Schmidt", email: "sam@example.com", avatarUrl: null, timeZone: "Europe/Berlin", active: true },
  { id: "alex", key: "JIRAUSER2", displayName: "Alex Weber", email: "alex@example.com", avatarUrl: null, timeZone: "Europe/Berlin", active: true },
  { id: "mira", key: "JIRAUSER3", displayName: "Mira Keller", email: "mira@example.com", avatarUrl: null, timeZone: "Europe/Berlin", active: true },
  { id: "jonas", key: "JIRAUSER4", displayName: "Jonas Brandt", email: "jonas@example.com", avatarUrl: null, timeZone: "Europe/Berlin", active: true },
  { id: "lea", key: "JIRAUSER5", displayName: "Lea Hoffmann", email: "lea@example.com", avatarUrl: null, timeZone: "Europe/Berlin", active: true },
  { id: "tim", key: "JIRAUSER6", displayName: "Tim Berger", email: "tim@example.com", avatarUrl: null, timeZone: "Europe/Berlin", active: true },
];

const member = (u: JiraUser, color: string, weeklyHours: number | null = null): Member => ({
  id: u.id,
  key: u.key,
  displayName: u.displayName,
  email: u.email,
  avatarUrl: null,
  weeklyHours,
  color,
});

const defaultSettings: Settings = {
  jira: { baseUrl: "https://jira.example.com", authMethod: "browserSession", username: "Sam Schmidt" },
  selection: { projectKeys: [] },
  teams: [
    {
      id: "team-1",
      name: "Team Horizon",
      members: [member(users[0], "#2f6fed"), member(users[1], "#17956b"), member(users[2], "#e8572a"), member(users[3], "#8b5cf6", 30)],
    },
    { id: "team-2", name: "Plattform", members: [member(users[4], "#ec4899"), member(users[5], "#0ea5e9"), member(users[0], "#2f6fed")] },
  ],
  work: { hoursPerWeekday: [8, 8, 8, 8, 8, 0, 0], firstDayOfWeek: 0, timeFormat: "hoursMinutes", showTargets: true },
  refresh: { auto: true, intervalMinutes: 30 },
  behavior: { theme: "system" },
  tempo: { source: "jiraWorklogs", cloudApiBase: "https://api.tempo.io/4", cloudTokenSet: false },
  projectColors: { WEB: "#3b82f6", API: "#10b981", OPS: "#f59e0b" },
};

let settings: Settings = structuredClone(defaultSettings);
let authenticated = true;

const projects: Project[] = [
  { id: "1", key: "WEB", name: "Web-Portal", avatarUrl: null, color: "#3b82f6" },
  { id: "2", key: "API", name: "Backend API", avatarUrl: null, color: "#10b981" },
  { id: "3", key: "OPS", name: "Betrieb", avatarUrl: null, color: "#f59e0b" },
  { id: "4", key: "HR", name: "HR-Portal", avatarUrl: null, color: "#8b5cf6" },
];

interface MockIssue {
  key: string;
  summary: string;
  projectKey: string;
  issueType: string;
  status: string;
  statusCategory: string;
  epicKey: string | null;
  originalEstimateS: number | null;
  weights: Record<string, number>; // member id -> affinity
}

const issues: MockIssue[] = [
  { key: "WEB-100", summary: "Dashboard-Relaunch", projectKey: "WEB", issueType: "Epic", status: "In Arbeit", statusCategory: "indeterminate", epicKey: null, originalEstimateS: null, weights: {} },
  { key: "WEB-101", summary: "Login-Seite: Fehlermeldung bei abgelaufener Session", projectKey: "WEB", issueType: "Bug", status: "Erledigt", statusCategory: "done", epicKey: null, originalEstimateS: 7200, weights: { me: 3, alex: 1 } },
  { key: "WEB-104", summary: "Dashboard-Widgets per Drag & Drop anordnen", projectKey: "WEB", issueType: "Story", status: "In Arbeit", statusCategory: "indeterminate", epicKey: "WEB-100", originalEstimateS: 28800, weights: { me: 4, mira: 2 } },
  { key: "WEB-105", summary: "Widget-Layout speichern", projectKey: "WEB", issueType: "Sub-task", status: "In Arbeit", statusCategory: "indeterminate", epicKey: "WEB-100", originalEstimateS: 7200, weights: { me: 2 } },
  { key: "WEB-110", summary: "Barrierefreiheit: Kontraste im Dunkelmodus prüfen", projectKey: "WEB", issueType: "Task", status: "Review", statusCategory: "indeterminate", epicKey: "WEB-100", originalEstimateS: 10800, weights: { alex: 4, jonas: 1 } },
  { key: "WEB-118", summary: "Onboarding-Tour für neue Nutzer", projectKey: "WEB", issueType: "Story", status: "To Do", statusCategory: "new", epicKey: "WEB-100", originalEstimateS: 36000, weights: { mira: 3, alex: 1 } },
  { key: "API-70", summary: "Public API v2", projectKey: "API", issueType: "Epic", status: "In Arbeit", statusCategory: "indeterminate", epicKey: null, originalEstimateS: null, weights: {} },
  { key: "API-77", summary: "Rate-Limiting für /search Endpunkt", projectKey: "API", issueType: "Story", status: "Erledigt", statusCategory: "done", epicKey: "API-70", originalEstimateS: 14400, weights: { alex: 3, jonas: 3 } },
  { key: "API-80", summary: "Worklog-Webhook an Tempo weiterleiten", projectKey: "API", issueType: "Task", status: "In Arbeit", statusCategory: "indeterminate", epicKey: "API-70", originalEstimateS: 5400, weights: { jonas: 4, me: 1 } },
  { key: "API-81", summary: "Flaky Test: SessionRefreshIT", projectKey: "API", issueType: "Bug", status: "In Arbeit", statusCategory: "indeterminate", epicKey: null, originalEstimateS: 3600, weights: { alex: 2, mira: 1 } },
  { key: "API-85", summary: "OpenAPI-Spezifikation für v2 generieren", projectKey: "API", issueType: "Story", status: "Review", statusCategory: "indeterminate", epicKey: "API-70", originalEstimateS: 21600, weights: { mira: 4, jonas: 2 } },
  { key: "OPS-12", summary: "Zertifikat jira.example.com erneuern", projectKey: "OPS", issueType: "Task", status: "Erledigt", statusCategory: "done", epicKey: null, originalEstimateS: 3600, weights: { me: 1, alex: 1 } },
  { key: "OPS-15", summary: "Monitoring-Alarme für Worker-Queue", projectKey: "OPS", issueType: "Task", status: "In Arbeit", statusCategory: "indeterminate", epicKey: null, originalEstimateS: 14400, weights: { jonas: 2, me: 1 } },
  { key: "OPS-19", summary: "Daily Standup / Meetings", projectKey: "OPS", issueType: "Task", status: "In Arbeit", statusCategory: "indeterminate", epicKey: null, originalEstimateS: null, weights: { me: 2, alex: 2, mira: 2, jonas: 2, lea: 2, tim: 2 } },
  { key: "HR-301", summary: "Urlaubsantrag: Vertretungsregel", projectKey: "HR", issueType: "Story", status: "In Arbeit", statusCategory: "indeterminate", epicKey: null, originalEstimateS: 18000, weights: { lea: 4, tim: 2 } },
  { key: "HR-305", summary: "Gehaltsabrechnung: PDF-Export", projectKey: "HR", issueType: "Task", status: "To Do", statusCategory: "new", epicKey: null, originalEstimateS: 10800, weights: { tim: 4, lea: 1 } },
];

interface MockWorklog {
  id: string;
  issueKey: string;
  authorId: string;
  started: Date;
  date: string;
  seconds: number;
  comment: string;
}

function mulberry32(seed: number) {
  return () => {
    seed |= 0;
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function dayKey(d: Date): string {
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

const comments = ["", "", "Analyse", "Umsetzung", "Review", "Tests", "Abstimmung mit Team", "Bugfix", "Doku", "Pairing"];

function generateWorklogs(): MockWorklog[] {
  const rnd = mulberry32(42);
  const out: MockWorklog[] = [];
  const today = new Date();
  const perDay: Record<string, number> = { me: 7.8, alex: 7.6, mira: 7.9, jonas: 5.8, lea: 7.4, tim: 7.7 };
  let id = 1000;
  for (let offset = 200; offset >= 0; offset--) {
    const day = new Date(today.getFullYear(), today.getMonth(), today.getDate() - offset);
    const wd = (day.getDay() + 6) % 7;
    if (wd >= 5) continue;
    for (const u of users) {
      if (rnd() < 0.09) continue; // day off / sick
      if (u.id === "jonas" && wd === 4) continue; // part-time: no Fridays
      const candidates = issues.filter((i) => (i.weights[u.id] ?? 0) > 0);
      const targetHours = perDay[u.id] * (0.75 + rnd() * 0.45);
      let remaining = Math.round(targetHours * 4) / 4;
      let hour = 8 + Math.floor(rnd() * 2);
      let minute = rnd() < 0.5 ? 0 : 30;
      while (remaining > 0 && hour < 19) {
        const total = candidates.reduce((a, i) => a + i.weights[u.id], 0);
        let pick = rnd() * total;
        let issue = candidates[0];
        for (const c of candidates) {
          pick -= c.weights[u.id];
          if (pick <= 0) {
            issue = c;
            break;
          }
        }
        const chunk = Math.min(remaining, Math.max(0.25, Math.round((0.5 + rnd() * 3) * 4) / 4));
        const started = new Date(day.getFullYear(), day.getMonth(), day.getDate(), hour, minute);
        out.push({ id: String(id++), issueKey: issue.key, authorId: u.id, started, date: dayKey(started), seconds: Math.round(chunk * 3600), comment: comments[Math.floor(rnd() * comments.length)] });
        remaining -= chunk;
        const advance = chunk + (rnd() < 0.3 ? 0.5 : 0);
        hour += Math.floor(advance);
        minute += Math.round((advance % 1) * 60);
        if (minute >= 60) {
          hour++;
          minute -= 60;
        }
      }
    }
  }
  return out;
}

const worklogs = generateWorklogs();
const coverage = new Map<string, Set<string>>();
for (const u of users) {
  const set = new Set<string>();
  const today = new Date();
  for (let offset = 400; offset >= 0; offset--) set.add(dayKey(new Date(today.getFullYear(), today.getMonth(), today.getDate() - offset)));
  coverage.set(u.id, set);
}

let fetchStatus: FetchStatus = { inProgress: false, progress: null, lastSuccess: new Date(Date.now() - 12 * 60000).toISOString(), lastError: null, lastRequest: null, lastOutcome: null };

const delay = (ms = 120) => new Promise((r) => setTimeout(r, ms));

function authStatus(): AuthStatus {
  return {
    configured: !!settings.jira.baseUrl,
    authenticated,
    method: authenticated ? "cookies" : null,
    user: authenticated ? users[0] : null,
    server: authenticated ? { baseUrl: settings.jira.baseUrl, version: "10.3.26", deployment: "dataCenter", serverTitle: "Example Jira" } : null,
    message: authenticated ? null : "Nicht angemeldet",
  };
}

function allMembers(): Member[] {
  const seen = new Set<string>();
  const out: Member[] = [];
  for (const t of settings.teams) for (const m of t.members) if (!seen.has(m.id)) { seen.add(m.id); out.push(m); }
  return out;
}

function membersByIds(ids: string[]): Member[] {
  const all = allMembers();
  return ids.map((id) => all.find((m) => m.id === id)).filter((m): m is Member => !!m);
}

function targetFor(m: Member, date: Date): number {
  const pattern = settings.work.hoursPerWeekday;
  const sum = pattern.reduce((a, b) => a + b, 0);
  const base = pattern[(date.getDay() + 6) % 7] ?? 0;
  const factor = m.weeklyHours != null && sum > 0 ? m.weeklyHours / sum : 1;
  return Math.round(base * factor * 3600);
}

function visibleWorklogs(from: string, to: string, memberIds: string[]): MockWorklog[] {
  const allowed = settings.selection.projectKeys;
  return worklogs.filter(
    (w) =>
      w.date >= from &&
      w.date <= to &&
      memberIds.includes(w.authorId) &&
      coverage.get(w.authorId)?.has(w.date) &&
      (!allowed.length || allowed.includes(w.issueKey.split("-")[0])),
  );
}

function projectColor(key: string): string | null {
  return settings.projectColors[key] ?? projects.find((p) => p.key === key)?.color ?? null;
}

function computeStats(from: string, to: string, memberIds: string[]): Stats {
  const members = membersByIds(memberIds);
  const today = dayKey(new Date());
  const days: DayStat[] = [];
  const memberTargets: Record<string, number> = {};
  const fromD = new Date(from + "T00:00:00");
  const toD = new Date(to + "T00:00:00");
  for (let d = new Date(fromD); d <= toD; d.setDate(d.getDate() + 1)) {
    const key = dayKey(d);
    let target = 0;
    if (key <= today) for (const m of members) { const t = targetFor(m, d); memberTargets[m.id] = (memberTargets[m.id] ?? 0) + t; target += t; }
    days.push({ date: key, seconds: 0, targetSeconds: target, byMember: {} });
  }
  const dayMap = new Map(days.map((d) => [d.date, d]));
  const logs = visibleWorklogs(from, to, memberIds);
  const byProject = new Map<string, number>();
  const byType = new Map<string, number>();
  const byWeekday = [0, 0, 0, 0, 0, 0, 0];
  const issueAcc = new Map<string, { seconds: number; count: number; byMember: Record<string, number> }>();
  const memberAcc = new Map<string, { seconds: number; count: number; issues: Set<string>; days: Set<string>; last: Date | null; byProject: Map<string, number> }>();
  const loggedDays = new Set<string>();
  let total = 0;
  for (const w of logs) {
    const issue = issues.find((i) => i.key === w.issueKey);
    const project = issue?.projectKey ?? w.issueKey.split("-")[0];
    total += w.seconds;
    loggedDays.add(w.date);
    const day = dayMap.get(w.date);
    if (day) { day.seconds += w.seconds; day.byMember[w.authorId] = (day.byMember[w.authorId] ?? 0) + w.seconds; }
    byWeekday[(new Date(w.date + "T00:00:00").getDay() + 6) % 7] += w.seconds;
    byProject.set(project, (byProject.get(project) ?? 0) + w.seconds);
    const type = issue?.issueType ?? "Unbekannt";
    byType.set(type, (byType.get(type) ?? 0) + w.seconds);
    const ma = memberAcc.get(w.authorId) ?? { seconds: 0, count: 0, issues: new Set(), days: new Set(), last: null, byProject: new Map() };
    ma.seconds += w.seconds; ma.count++; ma.issues.add(w.issueKey); ma.days.add(w.date);
    if (!ma.last || w.started > ma.last) ma.last = w.started;
    ma.byProject.set(project, (ma.byProject.get(project) ?? 0) + w.seconds);
    memberAcc.set(w.authorId, ma);
    const ia = issueAcc.get(w.issueKey) ?? { seconds: 0, count: 0, byMember: {} };
    ia.seconds += w.seconds; ia.count++; ia.byMember[w.authorId] = (ia.byMember[w.authorId] ?? 0) + w.seconds;
    issueAcc.set(w.issueKey, ia);
  }
  const groups = (map: Map<string, number>): GroupStat[] =>
    [...map.entries()]
      .map(([key, seconds]) => ({ key, label: projects.find((p) => p.key === key)?.name ?? key, seconds, projectKey: key, color: projectColor(key) }))
      .sort((a, b) => b.seconds - a.seconds || a.key.localeCompare(b.key));
  const memberStats: MemberStat[] = members.map((m) => {
    const a = memberAcc.get(m.id);
    const daysLogged = a?.days.size ?? 0;
    return {
      id: m.id,
      displayName: m.displayName,
      color: m.color,
      seconds: a?.seconds ?? 0,
      targetSeconds: memberTargets[m.id] ?? 0,
      worklogCount: a?.count ?? 0,
      issueCount: a?.issues.size ?? 0,
      daysLogged,
      avgPerLoggedDaySeconds: daysLogged ? Math.floor((a?.seconds ?? 0) / daysLogged) : 0,
      lastWorklog: a?.last ? a.last.toISOString() : null,
      byProject: groups(a?.byProject ?? new Map()),
    };
  });
  const byIssue: IssueStat[] = [...issueAcc.entries()]
    .map(([key, a]) => {
      const i = issues.find((x) => x.key === key);
      const project = i?.projectKey ?? key.split("-")[0];
      const allTime = worklogs.filter((w) => w.issueKey === key).reduce((s, w) => s + w.seconds, 0);
      return {
        key,
        summary: i?.summary ?? "",
        projectKey: project,
        issueType: i?.issueType ?? "",
        status: i?.status ?? "",
        statusCategory: i?.statusCategory ?? "undefined",
        parentKey: null,
        epicKey: i?.epicKey ?? null,
        assignee: null,
        seconds: a.seconds,
        worklogCount: a.count,
        byMember: a.byMember,
        originalEstimateS: i?.originalEstimateS ?? null,
        timeSpentS: allTime + 5400,
        color: projectColor(project),
      };
    })
    .sort((a, b) => b.seconds - a.seconds || a.key.localeCompare(b.key));
  return {
    from,
    to,
    totalSeconds: total,
    targetSeconds: days.reduce((a, d) => a + d.targetSeconds, 0),
    worklogCount: logs.length,
    issueCount: byIssue.length,
    daysLogged: loggedDays.size,
    workDays: days.filter((d) => d.targetSeconds > 0).length,
    members: memberStats,
    days,
    byProject: groups(byProject),
    byIssue,
    byIssueType: [...byType.entries()].map(([key, seconds]) => ({ key, label: key, seconds, projectKey: null, color: null })).sort((a, b) => b.seconds - a.seconds),
    byWeekday,
  };
}

function rows(from: string, to: string, memberIds: string[], issueKey: string | null, summarize: boolean): ExportRow[] {
  const members = allMembers();
  let list = visibleWorklogs(from, to, memberIds)
    .filter((w) => !issueKey || w.issueKey === issueKey)
    .map((w) => {
      const i = issues.find((x) => x.key === w.issueKey);
      return {
        date: w.date,
        started: w.started.toISOString(),
        memberId: w.authorId,
        member: members.find((m) => m.id === w.authorId)?.displayName ?? w.authorId,
        projectKey: i?.projectKey ?? w.issueKey.split("-")[0],
        issueKey: w.issueKey,
        summary: i?.summary ?? "",
        issueType: i?.issueType ?? "",
        seconds: w.seconds,
        comment: w.comment,
        worklogIds: [w.id],
      } satisfies ExportRow;
    })
    .sort((a, b) => a.started.localeCompare(b.started));
  if (summarize) {
    const map = new Map<string, ExportRow>();
    for (const r of list) {
      const k = `${r.date}|${r.memberId}|${r.issueKey}`;
      const g = map.get(k);
      if (g) {
        g.seconds += r.seconds;
        g.worklogIds.push(...r.worklogIds);
        if (r.comment && !g.comment.includes(r.comment)) g.comment = g.comment ? `${g.comment} | ${r.comment}` : r.comment;
      } else map.set(k, { ...r, worklogIds: [...r.worklogIds] });
    }
    list = [...map.values()];
  }
  return list;
}

function coverageInfo(from: string, to: string, memberIds: string[]): CoverageInfo {
  const fromD = new Date(from + "T00:00:00");
  const toD = new Date(to + "T00:00:00");
  let total = 0;
  let covered = 0;
  for (let d = new Date(fromD); d <= toD; d.setDate(d.getDate() + 1)) {
    total++;
    const key = dayKey(d);
    if (!memberIds.length || memberIds.every((id) => coverage.get(id)?.has(key))) covered++;
  }
  return { totalDays: total, coveredDays: covered, oldestFetch: fetchStatus.lastSuccess, newestFetch: fetchStatus.lastSuccess };
}

const cacheInfo = (): CacheInfo => ({ worklogs: worklogs.length, issues: issues.length, coverageDays: [...coverage.values()].reduce((a, s) => a + s.size, 0), databaseBytes: 2_400_000 });

export const mockApi = {
  getSettings: async () => structuredClone(settings),
  saveSettings: async (s: Settings) => {
    settings = structuredClone(s);
    mockEmit("settings-changed", structuredClone(settings));
    return structuredClone(settings);
  },
  getAuthStatus: async () => authStatus(),
  startBrowserLogin: async () => {
    mockEmit("login-window", "opened");
    mockEmit("login-progress", "Warte auf Anmeldung im Fenster …");
    await delay(1500);
    mockEmit("login-window", "closed");
    authenticated = true;
    mockEmit("auth-changed", authStatus());
    return authStatus();
  },
  cancelBrowserLogin: async () => {},
  loginWithPassword: async () => {
    await delay(600);
    authenticated = true;
    mockEmit("auth-changed", authStatus());
    return authStatus();
  },
  loginWithToken: async () => {
    await delay(600);
    authenticated = true;
    mockEmit("auth-changed", authStatus());
    return authStatus();
  },
  logout: async () => {
    authenticated = false;
    mockEmit("auth-changed", authStatus());
  },
  searchProjects: async (query: string) => {
    await delay();
    const q = query.toLowerCase();
    return projects.filter((p) => !q || p.key.toLowerCase().includes(q) || p.name.toLowerCase().includes(q)).map((p) => ({ ...p, color: projectColor(p.key) }));
  },
  listProjects: async () => projects.map((p) => ({ ...p, color: projectColor(p.key) })),
  setProjectSelection: async (ps: Project[]) => {
    settings.selection.projectKeys = ps.map((p) => p.key);
    for (const p of ps) if (!settings.projectColors[p.key]) settings.projectColors[p.key] = p.color ?? "#64748b";
    mockEmit("settings-changed", structuredClone(settings));
    mockEmit("data-changed");
    return structuredClone(settings);
  },
  setProjectColor: async (key: string, color: string | null) => {
    if (color) settings.projectColors[key] = color;
    else delete settings.projectColors[key];
    mockEmit("settings-changed", structuredClone(settings));
    mockEmit("data-changed");
    return structuredClone(settings);
  },
  searchUsers: async (query: string) => {
    await delay(200);
    const q = query.trim().toLowerCase();
    return users.filter((u) => !q || u.displayName.toLowerCase().includes(q) || (u.email ?? "").toLowerCase().includes(q));
  },
  fetchWorklogs: async (from: string, to: string, memberIds: string[], force: boolean): Promise<FetchStatus> => {
    if (fetchStatus.inProgress) return { ...fetchStatus };
    const info = coverageInfo(from, to, memberIds);
    if (!force && info.coveredDays >= info.totalDays) {
      fetchStatus = { ...fetchStatus, lastRequest: { from, to, memberIds }, lastOutcome: { fetched: false, source: settings.tempo.source === "tempoCloud" ? "tempoCloud" : "jira", from, to, authors: 0, authorsWithWorklogs: 0, issues: 0, worklogs: 0, removed: 0, otherAuthors: [], unresolvedIssues: 0 } };
      return { ...fetchStatus };
    }
    fetchStatus = { ...fetchStatus, inProgress: true, progress: { phase: "issues", done: 0, total: 0 }, lastError: null, lastRequest: { from, to, memberIds } };
    mockEmit("fetch-status", { ...fetchStatus });
    await delay(500);
    const total = 14;
    for (let i = 1; i <= total; i++) {
      await delay(60);
      fetchStatus = { ...fetchStatus, progress: { phase: "worklogs", done: i, total } };
      mockEmit("fetch-status", { ...fetchStatus });
    }
    for (const id of memberIds) {
      const set = coverage.get(id) ?? new Set<string>();
      for (let d = new Date(from + "T00:00:00"); d <= new Date(to + "T00:00:00"); d.setDate(d.getDate() + 1)) set.add(dayKey(d));
      coverage.set(id, set);
    }
    const n = visibleWorklogs(from, to, memberIds).length;
    fetchStatus = { inProgress: false, progress: null, lastSuccess: new Date().toISOString(), lastError: null, lastRequest: { from, to, memberIds }, lastOutcome: { fetched: true, source: settings.tempo.source === "tempoCloud" ? "tempoCloud" : "jira", from, to, authors: memberIds.length, authorsWithWorklogs: new Set(visibleWorklogs(from, to, memberIds).map((w) => w.authorId)).size, issues: total, worklogs: n, removed: 0, otherAuthors: settings.tempo.source === "tempoCloud" ? [] : [{ id: "app", name: "Tempo Timesheets", count: 57 }], unresolvedIssues: 0 } };
    mockEmit("fetch-status", { ...fetchStatus });
    mockEmit("data-changed");
    return { ...fetchStatus };
  },
  getFetchStatus: async () => ({ ...fetchStatus }),
  getCoverage: async (from: string, to: string, memberIds: string[]) => coverageInfo(from, to, memberIds),
  getStats: async (from: string, to: string, memberIds: string[]) => computeStats(from, to, memberIds),
  listWorklogs: async (from: string, to: string, memberIds: string[], issueKey: string | null, summarize: boolean) => rows(from, to, memberIds, issueKey, summarize),
  exportWorklogs: async () => 0,
  getIssueUrl: async (key: string) => `${settings.jira.baseUrl}/browse/${key}`,
  clearCache: async () => {
    for (const set of coverage.values()) set.clear();
    fetchStatus = { ...fetchStatus, lastSuccess: null, lastOutcome: null, lastRequest: null };
    mockEmit("fetch-status", { ...fetchStatus });
    mockEmit("data-changed");
    return cacheInfo();
  },
  setTempoCloudToken: async (token: string | null) => {
    await delay(400);
    settings.tempo.cloudTokenSet = !!token;
    mockEmit("settings-changed", structuredClone(settings));
    return structuredClone(settings);
  },
  getAppInfo: async (): Promise<AppInfo> => ({
    version: "0.1.0-mock",
    platform: "browser",
    configDir: "~/Library/Application Support/YaTTAT (Mock)",
    dataDir: "~/Library/Application Support/YaTTAT (Mock)",
    logDir: "~/Library/Application Support/YaTTAT/logs (Mock)",
    secretBackend: "keyring",
    cache: cacheInfo(),
  }),
};

declare global {
  interface Window {
    __yattatMock?: typeof mockApi;
  }
}
if (typeof window !== "undefined") window.__yattatMock = mockApi;
