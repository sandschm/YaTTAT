// Typed wrappers around the Tauri commands. Types mirror the Rust structs (serde camelCase).
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Deployment = "unknown" | "cloud" | "dataCenter";
export type AuthMethod = "browserSession" | "password" | "personalAccessToken" | "apiToken";
export type TimeFormat = "hoursMinutes" | "decimal";
export type Theme = "system" | "light" | "dark";
export type LanguageSetting = "system" | "de" | "en";
export type WorklogSource = "jiraWorklogs" | "tempoCloud";

export interface Member {
  /** Jira user id (Cloud: accountId, Data Center: username). */
  id: string;
  key: string | null;
  displayName: string;
  email: string | null;
  avatarUrl: string | null;
  /** Contract hours per week; scales the weekday pattern. null = full pattern. */
  weeklyHours: number | null;
  color: string | null;
}

export interface Team {
  id: string;
  name: string;
  members: Member[];
}

export interface Settings {
  jira: { baseUrl: string; authMethod: AuthMethod; username: string | null };
  selection: { projectKeys: string[] };
  teams: Team[];
  work: { hoursPerWeekday: number[]; firstDayOfWeek: number; timeFormat: TimeFormat; showTargets: boolean };
  refresh: { auto: boolean; intervalMinutes: number };
  behavior: { theme: Theme; language: LanguageSetting };
  tempo: { source: WorklogSource; cloudApiBase: string; cloudTokenSet: boolean };
  projectColors: Record<string, string>;
}

export interface ServerInfo {
  baseUrl: string;
  version: string;
  deployment: Deployment;
  serverTitle: string | null;
}

export interface JiraUser {
  id: string;
  key: string | null;
  displayName: string;
  email: string | null;
  avatarUrl: string | null;
  timeZone: string | null;
  active: boolean;
}

export interface AuthStatus {
  configured: boolean;
  authenticated: boolean;
  method: string | null;
  user: JiraUser | null;
  server: ServerInfo | null;
  message: string | null;
}

export interface Project {
  id: string;
  key: string;
  name: string;
  avatarUrl: string | null;
  color: string | null;
}

export interface FetchProgress {
  phase: "issues" | "worklogs" | string;
  done: number;
  total: number;
}

export interface AuthorCount {
  id: string;
  name: string;
  count: number;
}

export interface FetchOutcome {
  fetched: boolean;
  /** "jira" or "tempoCloud" */
  source: string;
  from: string;
  to: string;
  authors: number;
  /** Requested members (ids) without any worklog in the fetched range. */
  authorsWithoutWorklogs: string[];
  issues: number;
  worklogs: number;
  removed: number;
  /** Worklogs in the range by people who are not members (Jira source), e.g. the Tempo app. */
  otherAuthors: AuthorCount[];
  /** Tempo worklogs whose issue could not be read from Jira. */
  unresolvedIssues: number;
}

export interface FetchStatus {
  inProgress: boolean;
  progress: FetchProgress | null;
  lastSuccess: string | null;
  lastError: string | null;
  lastRequest: { from: string; to: string; memberIds: string[] } | null;
  lastOutcome: FetchOutcome | null;
}

export interface DayDiff {
  date: string;
  jiraSeconds: number;
  tempoSeconds: number;
}

export interface MemberComparison {
  id: string;
  jiraSeconds: number;
  tempoSeconds: number;
  jiraWorklogs: number;
  tempoWorklogs: number;
  differingDays: DayDiff[];
}

/** Result of reading the same range from Jira and Tempo. */
export interface SourceComparison {
  from: string;
  to: string;
  jiraSeconds: number;
  tempoSeconds: number;
  jiraIssues: number;
  tempoIssues: number;
  members: MemberComparison[];
  jiraOtherAuthors: AuthorCount[];
  tempoUnresolvedIssues: number;
}

export interface CompareProgress {
  /** "jira", "tempo" or "done" */
  source: string;
  phase: string;
  done: number;
  total: number;
}

export interface CoverageInfo {
  totalDays: number;
  coveredDays: number;
  oldestFetch: string | null;
  newestFetch: string | null;
}

export interface GroupStat {
  key: string;
  label: string;
  seconds: number;
  projectKey: string | null;
  color: string | null;
}

export interface DayStat {
  date: string;
  seconds: number;
  targetSeconds: number;
  byMember: Record<string, number>;
}

export interface MemberStat {
  id: string;
  displayName: string;
  color: string | null;
  seconds: number;
  targetSeconds: number;
  worklogCount: number;
  issueCount: number;
  daysLogged: number;
  avgPerLoggedDaySeconds: number;
  lastWorklog: string | null;
  byProject: GroupStat[];
}

export interface IssueStat {
  key: string;
  summary: string;
  projectKey: string;
  issueType: string;
  status: string;
  statusCategory: string;
  parentKey: string | null;
  epicKey: string | null;
  assignee: string | null;
  seconds: number;
  worklogCount: number;
  byMember: Record<string, number>;
  originalEstimateS: number | null;
  timeSpentS: number | null;
  color: string | null;
}

export interface Stats {
  from: string;
  to: string;
  totalSeconds: number;
  targetSeconds: number;
  worklogCount: number;
  issueCount: number;
  daysLogged: number;
  workDays: number;
  members: MemberStat[];
  days: DayStat[];
  byProject: GroupStat[];
  byIssue: IssueStat[];
  byIssueType: GroupStat[];
  byWeekday: number[];
}

export interface ExportRow {
  date: string;
  started: string;
  memberId: string;
  member: string;
  projectKey: string;
  issueKey: string;
  summary: string;
  issueType: string;
  seconds: number;
  comment: string;
  worklogIds: string[];
}

export interface CacheInfo {
  worklogs: number;
  issues: number;
  coverageDays: number;
  databaseBytes: number;
}

export interface AppInfo {
  version: string;
  platform: string;
  configDir: string;
  dataDir: string;
  logDir: string;
  secretBackend: "keyring" | "file";
  cache: CacheInfo;
}

export const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tauriApi = {
  // settings
  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<Settings>("save_settings", { settings }),
  // auth
  getAuthStatus: () => invoke<AuthStatus>("get_auth_status"),
  startBrowserLogin: () => invoke<AuthStatus>("start_browser_login"),
  cancelBrowserLogin: () => invoke<void>("cancel_browser_login"),
  loginWithPassword: (username: string, password: string) => invoke<AuthStatus>("login_with_password", { username, password }),
  loginWithToken: (kind: "pat" | "apiToken", user: string | null, token: string) =>
    invoke<AuthStatus>("login_with_token", { kind, user, token }),
  logout: () => invoke<void>("logout"),
  // jira metadata
  searchProjects: (query: string) => invoke<Project[]>("search_projects", { query }),
  listProjects: () => invoke<Project[]>("list_projects"),
  setProjectSelection: (projects: Project[]) => invoke<Settings>("set_project_selection", { projects }),
  setProjectColor: (key: string, color: string | null) => invoke<Settings>("set_project_color", { key, color }),
  searchUsers: (query: string) => invoke<JiraUser[]>("search_users", { query }),
  // data
  fetchWorklogs: (from: string, to: string, memberIds: string[], force: boolean) =>
    invoke<FetchStatus>("fetch_worklogs", { from, to, memberIds, force }),
  getFetchStatus: () => invoke<FetchStatus>("get_fetch_status"),
  getCoverage: (from: string, to: string, memberIds: string[]) => invoke<CoverageInfo>("get_coverage", { from, to, memberIds }),
  getStats: (from: string, to: string, memberIds: string[]) => invoke<Stats>("get_stats", { from, to, memberIds }),
  listWorklogs: (from: string, to: string, memberIds: string[], issueKey: string | null, summarize: boolean) =>
    invoke<ExportRow[]>("list_worklogs", { from, to, memberIds, issueKey, summarize }),
  exportWorklogs: (from: string, to: string, memberIds: string[], format: "csv" | "json", summarize: boolean, path: string) =>
    invoke<number>("export_worklogs", { from, to, memberIds, format, summarize, path }),
  getIssueUrl: (key: string) => invoke<string>("get_issue_url", { key }),
  clearCache: () => invoke<CacheInfo>("clear_cache"),
  // tempo
  setTempoCloudToken: (token: string | null) => invoke<Settings>("set_tempo_cloud_token", { token }),
  compareSources: (from: string, to: string, memberIds: string[]) => invoke<SourceComparison>("compare_sources", { from, to, memberIds }),
  // info
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
};

// Outside Tauri (plain browser during development) a mock backend with sample data is used so the
// UI can be exercised without Jira. The mock lives in ./mock.ts.
import { mockApi, mockListen } from "./mock";

export const api: typeof tauriApi = isTauri ? tauriApi : (mockApi as unknown as typeof tauriApi);

export type AppEvent = "auth-changed" | "settings-changed" | "fetch-status" | "data-changed" | "login-window" | "login-progress" | "compare-progress";

export function on<T = unknown>(event: AppEvent, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauri) {
    const off = mockListen(event, (p) => handler(p as T));
    return Promise.resolve(off);
  }
  return listen<T>(event, (e) => handler(e.payload));
}

/** Opens a URL in the system browser (Tauri) or a new tab (browser mock). */
export async function openExternal(url: string) {
  if (isTauri) {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } else {
    window.open(url, "_blank");
  }
}
