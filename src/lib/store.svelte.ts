// Global reactive state (Svelte 5 runes). One instance shared by all views.
import { api, isTauri, on, type AuthStatus, type CompareProgress, type FetchStatus, type Member, type Settings, type Team } from "./api";
import { addDays, startOfDay, startOfWeek, toDateKey } from "./format";
import { t, type Key } from "./i18n.svelte";

export type View = "team" | "member" | "tickets" | "settings";
export type Preset = "thisWeek" | "lastWeek" | "thisMonth" | "lastMonth" | "thisQuarter" | "lastQuarter" | "thisYear" | "custom";

export const presets: { id: Preset; label: Key }[] = [
  { id: "thisWeek", label: "range.thisWeek" },
  { id: "lastWeek", label: "range.lastWeek" },
  { id: "thisMonth", label: "range.thisMonth" },
  { id: "lastMonth", label: "range.lastMonth" },
  { id: "thisQuarter", label: "range.thisQuarter" },
  { id: "lastQuarter", label: "range.lastQuarter" },
  { id: "thisYear", label: "range.thisYear" },
  { id: "custom", label: "range.custom" },
];

export interface Toast {
  id: number;
  message: string;
  kind: "info" | "success" | "error";
}

export interface ConfirmRequest {
  title: string;
  message: string;
  confirmLabel: string;
  danger: boolean;
  resolve: (ok: boolean) => void;
}

const emptyFetch: FetchStatus = { inProgress: false, progress: null, lastSuccess: null, lastError: null, lastRequest: null, lastOutcome: null };

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key);
    return raw ? (JSON.parse(raw) as T) : fallback;
  } catch {
    return fallback;
  }
}

function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // storage is a convenience only
  }
}

function sameIds(a: string[], b: string[]): boolean {
  return a.length === b.length && [...a].sort().every((x, i) => x === [...b].sort()[i]);
}

export function computeRange(preset: Preset, customFrom: string, customTo: string, firstDay: number): { from: string; to: string } {
  const today = startOfDay(new Date());
  const y = today.getFullYear();
  const m = today.getMonth();
  const q = Math.floor(m / 3);
  switch (preset) {
    case "thisWeek": {
      const s = startOfWeek(today, firstDay);
      return { from: toDateKey(s), to: toDateKey(addDays(s, 6)) };
    }
    case "lastWeek": {
      const s = addDays(startOfWeek(today, firstDay), -7);
      return { from: toDateKey(s), to: toDateKey(addDays(s, 6)) };
    }
    case "thisMonth":
      return { from: toDateKey(new Date(y, m, 1)), to: toDateKey(new Date(y, m + 1, 0)) };
    case "lastMonth":
      return { from: toDateKey(new Date(y, m - 1, 1)), to: toDateKey(new Date(y, m, 0)) };
    case "thisQuarter":
      return { from: toDateKey(new Date(y, q * 3, 1)), to: toDateKey(new Date(y, q * 3 + 3, 0)) };
    case "lastQuarter":
      return { from: toDateKey(new Date(y, q * 3 - 3, 1)), to: toDateKey(new Date(y, q * 3, 0)) };
    case "thisYear":
      return { from: toDateKey(new Date(y, 0, 1)), to: toDateKey(new Date(y, 11, 31)) };
    default:
      return { from: customFrom, to: customTo < customFrom ? customFrom : customTo };
  }
}

class AppStore {
  view = $state<View>("team");
  settings = $state<Settings | null>(null);
  auth = $state<AuthStatus>({ configured: false, authenticated: false, method: null, user: null, server: null, message: null });
  fetchStatus = $state<FetchStatus>(emptyFetch);
  teamId = $state<string | null>(load("yattat.teamId", null));
  memberId = $state<string | null>(load("yattat.memberId", null));
  preset = $state<Preset>(load<Preset>("yattat.preset", "thisMonth"));
  customFrom = $state(load("yattat.customFrom", toDateKey(addDays(new Date(), -30))));
  customTo = $state(load("yattat.customTo", toDateKey(new Date())));
  /** Bumped whenever the cache changed; views reload their data. */
  dataVersion = $state(0);
  toasts = $state<Toast[]>([]);
  confirmRequest = $state<ConfirmRequest | null>(null);
  loginWindowOpen = $state(false);
  loginProgress = $state("");
  compareProgress = $state<CompareProgress | null>(null);
  ready = $state(false);
  autoSwitched = false;

  private toastId = 1;

  timeFormat = $derived(this.settings?.work.timeFormat ?? "hoursMinutes");
  firstDay = $derived(this.settings?.work.firstDayOfWeek ?? 0);
  showTargets = $derived(this.settings?.work.showTargets ?? true);
  connected = $derived(this.auth.authenticated);
  teams = $derived<Team[]>(this.settings?.teams ?? []);
  team = $derived<Team | null>(this.teams.find((t) => t.id === this.teamId) ?? this.teams[0] ?? null);
  members = $derived<Member[]>(this.team?.members ?? []);
  memberIds = $derived(this.members.map((m) => m.id));
  member = $derived<Member | null>(this.members.find((m) => m.id === this.memberId) ?? this.members[0] ?? null);
  range = $derived.by(() => computeRange(this.preset, this.customFrom, this.customTo, this.firstDay));
  /** Members of all teams, each once. */
  allMembers = $derived.by(() => {
    const seen = new Set<string>();
    const out: Member[] = [];
    for (const t of this.teams) for (const m of t.members) if (!seen.has(m.id)) { seen.add(m.id); out.push(m); }
    return out;
  });

  async init() {
    const [settings, auth, fetchStatus] = await Promise.all([api.getSettings(), api.getAuthStatus(), api.getFetchStatus()]);
    this.settings = settings;
    this.auth = auth;
    this.fetchStatus = fetchStatus;
    if (!auth.configured || !auth.authenticated) {
      this.view = "settings";
      this.autoSwitched = true;
    }
    await Promise.all([
      on<AuthStatus>("auth-changed", (a) => {
        this.auth = a;
        if (a.message && !a.authenticated) this.toast(a.message, "error");
        // The stored session is restored asynchronously after start; jump back to the overview
        // once it is there, unless the user navigated to the settings on purpose.
        if (a.authenticated && this.autoSwitched && this.view === "settings") {
          this.view = this.members.length ? "team" : "settings";
          this.autoSwitched = false;
        }
      }),
      on<Settings>("settings-changed", (s) => (this.settings = s)),
      on<FetchStatus>("fetch-status", (f) => {
        const hadError = this.fetchStatus.lastError;
        const wasInProgress = this.fetchStatus.inProgress;
        this.fetchStatus = f;
        if (f.lastError && f.lastError !== hadError) this.toast(t("common.fetchFailed", { msg: f.lastError }), "error");
        // A request made while another fetch ran was dropped by the backend. Once the running
        // fetch is done, make sure the view that is displayed now is covered (no-op when it is).
        if (wasInProgress && !f.inProgress && this.ready && !f.lastError) {
          const r = f.lastRequest;
          const same = !!r && r.from === this.range.from && r.to === this.range.to && sameIds(r.memberIds, this.memberIds);
          if (!same || f.lastOutcome?.fetched) this.ensureData(false);
        }
      }),
      on<CompareProgress>("compare-progress", (p) => (this.compareProgress = p.source === "done" ? null : p)),
      on("data-changed", () => this.dataVersion++),
      on<string>("login-window", (s) => {
        this.loginWindowOpen = s === "opened";
        if (!this.loginWindowOpen) this.loginProgress = "";
      }),
      on<string>("login-progress", (s) => (this.loginProgress = s)),
    ]);
    if (!isTauri) {
      // Browser mock only: ?view=member opens a view directly, ?lang=en forces a language
      // (both used for screenshots).
      const params = new URLSearchParams(location.search);
      const requested = params.get("view");
      if (requested === "team" || requested === "member" || requested === "tickets" || requested === "settings") this.view = requested;
      const lang = params.get("lang");
      if ((lang === "de" || lang === "en") && this.settings) this.settings = { ...this.settings, behavior: { ...this.settings.behavior, language: lang } };
    }
    this.ready = true;
  }

  persist() {
    save("yattat.teamId", this.teamId);
    save("yattat.memberId", this.memberId);
    save("yattat.preset", this.preset);
    save("yattat.customFrom", this.customFrom);
    save("yattat.customTo", this.customTo);
  }

  /** Makes sure the cache covers the current range for the current team (no-op when cached). */
  async ensureData(force = false) {
    if (!this.connected || !this.memberIds.length) return;
    try {
      this.fetchStatus = await api.fetchWorklogs(this.range.from, this.range.to, this.memberIds, force);
    } catch (e) {
      this.toast(String(e), "error");
    }
  }

  memberById(id: string): Member | undefined {
    return this.allMembers.find((m) => m.id === id);
  }

  memberName(id: string): string {
    return this.memberById(id)?.displayName ?? id;
  }

  memberColor(id: string): string {
    return this.memberById(id)?.color ?? "#64748b";
  }

  projectColor(key: string): string {
    return this.settings?.projectColors[key] ?? "#64748b";
  }

  toast(message: string, kind: Toast["kind"] = "info", timeoutMs = 5000) {
    const id = this.toastId++;
    this.toasts = [...this.toasts, { id, message, kind }];
    setTimeout(() => this.dismissToast(id), timeoutMs);
    return id;
  }

  dismissToast(id: number) {
    this.toasts = this.toasts.filter((t) => t.id !== id);
  }

  confirm(title: string, message: string, confirmLabel = "OK", danger = false): Promise<boolean> {
    return new Promise((resolve) => {
      this.confirmRequest = { title, message, confirmLabel, danger, resolve };
    });
  }

  async saveSettings(patch: (s: Settings) => void) {
    if (!this.settings) return false;
    const next = structuredClone($state.snapshot(this.settings));
    patch(next);
    try {
      this.settings = await api.saveSettings(next);
      return true;
    } catch (e) {
      this.toast(String(e), "error");
      return false;
    }
  }
}

export const store = new AppStore();
