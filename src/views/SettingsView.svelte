<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type AppInfo, type AuthMethod, type JiraUser, type Member, type Project, type Settings, type SourceComparison, type Team } from "../lib/api";
  import { formatDateTime, formatHours, formatShortDate, formatSigned, weekdayNames } from "../lib/format";
  import { t, type Key } from "../lib/i18n.svelte";
  import Icon from "../components/Icon.svelte";
  import Avatar from "../components/Avatar.svelte";

  type Tab = "connection" | "projects" | "team" | "display" | "info";
  // Browser mock only: ?tab=team opens a settings tab directly (used for screenshots).
  const requestedTab = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window) ? new URLSearchParams(location.search).get("tab") : null;
  let tab = $state<Tab>((["connection", "projects", "team", "display", "info"].includes(requestedTab ?? "") ? requestedTab : store.connected ? "team" : "connection") as Tab);
  const tabs: { id: Tab; label: Key }[] = [
    { id: "connection", label: "tab.connection" },
    { id: "projects", label: "tab.projects" },
    { id: "team", label: "tab.team" },
    { id: "display", label: "tab.display" },
    { id: "info", label: "tab.info" },
  ];

  // ---- draft handling: edits go to a local copy and are saved debounced
  let draft = $state<Settings | null>(null);
  let dirty = $state(false);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const s = store.settings;
    if (s && !dirty) draft = structuredClone($state.snapshot(s));
  });

  function change(immediate = false) {
    dirty = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(saveNow, immediate ? 0 : 500);
  }
  async function saveNow() {
    if (!draft) return;
    try {
      const saved = await api.saveSettings($state.snapshot(draft));
      dirty = false;
      store.settings = saved;
    } catch (e) {
      store.toast(String(e), "error");
    }
  }

  // ---- connection
  let username = $state("");
  let password = $state("");
  let token = $state("");
  let email = $state("");
  let busy = $state(false);
  let authError = $state("");
  const auth = $derived(store.auth);
  const methodHelp: Record<AuthMethod, Key> = {
    browserSession: "conn.help.browserSession",
    password: "conn.help.password",
    personalAccessToken: "conn.help.personalAccessToken",
    apiToken: "conn.help.apiToken",
  };
  const methods: [AuthMethod, Key][] = [
    ["browserSession", "conn.methodBrowser"],
    ["password", "conn.methodPassword"],
    ["personalAccessToken", "conn.methodPat"],
    ["apiToken", "conn.methodApiToken"],
  ];
  const statusMethod: Record<string, Key> = { cookies: "status.methodCookies", basic: "status.methodBasic", bearer: "status.methodBearer", apiToken: "status.methodApiToken" };

  async function connect() {
    if (!draft) return;
    authError = "";
    busy = true;
    try {
      await saveNow();
      switch (draft.jira.authMethod) {
        case "browserSession":
          await api.startBrowserLogin();
          break;
        case "password":
          await api.loginWithPassword(username, password);
          password = "";
          break;
        case "personalAccessToken":
          await api.loginWithToken("pat", null, token);
          token = "";
          break;
        case "apiToken":
          await api.loginWithToken("apiToken", email, token);
          token = "";
          break;
      }
      store.auth = await api.getAuthStatus();
      store.toast(t("conn.signedInAs", { name: store.auth.user?.displayName ?? "?" }), "success");
      if (!store.members.length) tab = "team";
    } catch (e) {
      authError = String(e);
    } finally {
      busy = false;
    }
  }

  async function disconnect() {
    if (!(await store.confirm(t("conn.signOut"), t("conn.signOutText"), t("conn.signOut")))) return;
    await api.logout();
    store.auth = await api.getAuthStatus();
  }

  // ---- tempo
  let tempoToken = $state("");
  let tempoBusy = $state(false);
  async function saveTempoToken(clear = false) {
    tempoBusy = true;
    try {
      await saveNow();
      store.settings = await api.setTempoCloudToken(clear ? null : tempoToken);
      tempoToken = "";
      store.toast(t(clear ? "source.tokenRemoved" : "source.tokenSaved"), "success");
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      tempoBusy = false;
    }
  }
  const lastOutcome = $derived(store.fetchStatus.lastOutcome);
  const otherAuthorsTotal = $derived((lastOutcome?.otherAuthors ?? []).reduce((a, o) => a + o.count, 0));
  const missingNames = $derived((lastOutcome?.fetched ? lastOutcome.authorsWithoutWorklogs : []).map((id) => store.memberName(id)));

  // ---- source comparison
  let comparing = $state(false);
  let comparison = $state<SourceComparison | null>(null);
  let compareError = $state("");
  let openDiff = $state<string | null>(null);
  const tf = $derived(store.timeFormat);
  async function runCompare() {
    comparing = true;
    compareError = "";
    comparison = null;
    try {
      await saveNow();
      comparison = await api.compareSources(store.range.from, store.range.to, store.memberIds);
    } catch (e) {
      compareError = String(e);
    } finally {
      comparing = false;
      store.compareProgress = null;
    }
  }
  const compareText = $derived.by(() => {
    const p = store.compareProgress;
    if (!p) return t("common.loading");
    const src = p.source === "jira" ? "Jira" : "Tempo";
    if (p.phase === "issues") return p.total ? t("compare.progressIssues", { src, done: p.done, total: p.total }) : t("compare.progressIssuesSearch", { src });
    return p.total ? t("compare.progressWorklogs", { src, done: p.done, total: p.total }) : t("compare.progressWorklogsPlain", { src });
  });

  // ---- projects
  let projectQuery = $state("");
  let projectResults = $state<Project[]>([]);
  let projectsLoading = $state(false);
  let knownProjects = $state<Project[]>([]);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  const selectedKeys = $derived(store.settings?.selection.projectKeys ?? []);
  const selectedProjects = $derived(selectedKeys.map((k) => knownProjects.find((p) => p.key === k) ?? projectResults.find((p) => p.key === k) ?? { id: "", key: k, name: k, avatarUrl: null, color: null }));

  async function searchProjects() {
    if (!store.connected) return;
    projectsLoading = true;
    try {
      projectResults = await api.searchProjects(projectQuery);
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      projectsLoading = false;
    }
  }
  function onProjectQuery() {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(searchProjects, 350);
  }
  async function toggleProject(p: Project) {
    const isSelected = selectedKeys.includes(p.key);
    const next = isSelected ? selectedProjects.filter((x) => x.key !== p.key) : [...selectedProjects, p];
    try {
      store.settings = await api.setProjectSelection(next);
      knownProjects = await api.listProjects();
    } catch (e) {
      store.toast(String(e), "error");
    }
  }
  async function setColor(key: string, color: string) {
    try {
      store.settings = await api.setProjectColor(key, color);
    } catch (e) {
      store.toast(String(e), "error");
    }
  }
  $effect(() => {
    if (tab === "projects") {
      api.listProjects().then((p) => (knownProjects = p)).catch(() => {});
      if (store.connected) searchProjects();
    }
  });

  // ---- teams & members
  let selectedTeamId = $state<string | null>(store.team?.id ?? null);
  const selectedTeam = $derived<Team | null>(draft?.teams.find((t) => t.id === selectedTeamId) ?? draft?.teams[0] ?? null);
  let userQuery = $state("");
  let userResults = $state<JiraUser[]>([]);
  let usersLoading = $state(false);
  let userTimer: ReturnType<typeof setTimeout> | undefined;
  let userSearchDone = $state(false);
  const patternHours = $derived((draft?.work.hoursPerWeekday ?? []).reduce((a, b) => a + b, 0));

  function newId(prefix: string): string {
    const rnd = typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID().slice(0, 8) : Math.random().toString(36).slice(2, 10);
    return `${prefix}-${rnd}`;
  }
  function addTeam() {
    if (!draft) return;
    const team: Team = { id: newId("team"), name: t("teams.newName", { n: draft.teams.length + 1 }), members: [] };
    draft.teams = [...draft.teams, team];
    selectedTeamId = team.id;
    change();
  }
  async function removeTeam(team: Team) {
    if (!draft) return;
    if (!(await store.confirm(t("teams.delete"), t("teams.deleteText", { name: team.name, n: team.members.length }), t("common.delete"), true))) return;
    draft.teams = draft.teams.filter((x) => x.id !== team.id);
    selectedTeamId = draft.teams[0]?.id ?? null;
    if (store.teamId === team.id) store.teamId = selectedTeamId;
    change(true);
  }
  function renameTeam(team: Team, name: string) {
    team.name = name;
    change();
  }
  function selectTeam(team: Team) {
    selectedTeamId = team.id;
    userResults = [];
    userQuery = "";
    userSearchDone = false;
  }
  function memberInTeam(id: string): boolean {
    return !!selectedTeam?.members.some((m) => m.id === id);
  }
  function addMember(u: JiraUser) {
    if (!draft || !selectedTeam || memberInTeam(u.id)) return;
    // keep the color and hours a member already has in another team
    const known = store.memberById(u.id);
    const member: Member = {
      id: u.id,
      key: u.key,
      displayName: u.displayName,
      email: u.email,
      avatarUrl: u.avatarUrl,
      weeklyHours: known?.weeklyHours ?? null,
      color: known?.color ?? null,
    };
    selectedTeam.members = [...selectedTeam.members, member];
    change(true);
  }
  async function removeMember(m: Member) {
    if (!selectedTeam) return;
    if (!(await store.confirm(t("teams.removeMemberTitle"), t("teams.removeMemberText", { name: m.displayName, team: selectedTeam.name }), t("common.remove"), true))) return;
    selectedTeam.members = selectedTeam.members.filter((x) => x.id !== m.id);
    change(true);
  }
  function setWeeklyHours(m: Member, value: string) {
    const v = value.trim() === "" ? null : Number(value.replace(",", "."));
    m.weeklyHours = v != null && Number.isFinite(v) ? v : null;
    change();
  }
  function setMemberColor(m: Member, color: string) {
    // same member, same color in every team
    for (const team of draft?.teams ?? []) for (const x of team.members) if (x.id === m.id) x.color = color;
    change();
  }
  function onUserQuery() {
    clearTimeout(userTimer);
    userTimer = setTimeout(searchUsers, 300);
  }
  async function searchUsers() {
    if (!store.connected) return;
    usersLoading = true;
    try {
      userResults = await api.searchUsers(userQuery);
      userSearchDone = true;
    } catch (e) {
      store.toast(String(e), "error");
    } finally {
      usersLoading = false;
    }
  }
  function addMe() {
    if (auth.user) addMember(auth.user);
  }

  // ---- info
  let info = $state<AppInfo | null>(null);
  $effect(() => {
    if (tab === "info") api.getAppInfo().then((i) => (info = i)).catch(() => {});
  });
  async function clearCache() {
    if (!(await store.confirm(t("cache.clear"), t("cache.clearText"), t("cache.clear"), true))) return;
    try {
      const cache = await api.clearCache();
      if (info) info = { ...info, cache };
      store.toast(t("cache.cleared"), "success");
    } catch (e) {
      store.toast(String(e), "error");
    }
  }
  function formatBytes(n: number): string {
    if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
    return `${(n / 1024 / 1024).toFixed(1).replace(".", ",")} MB`;
  }

  const weekdays = $derived(weekdayNames());
</script>

<div class="view">
  <div class="view-header">
    <h1>{t("settings.title")}</h1>
    <div class="grow"></div>
    {#if dirty}<span class="faint">{t("settings.saving")}</span>{/if}
  </div>
  <div class="layout">
    <nav class="tabs">
      {#each tabs as item (item.id)}
        <button class:on={tab === item.id} type="button" onclick={() => (tab = item.id)}>{t(item.label)}</button>
      {/each}
    </nav>
    <div class="pane">
      {#if !draft}
        <p class="faint">{t("common.loading")}</p>
      {:else if tab === "connection"}
        <section class="card block">
          <h3>{t("conn.title")}</h3>
          <div class="field">
            <label for="jira-url">{t("conn.url")}</label>
            <input id="jira-url" type="url" placeholder={t("conn.urlPlaceholder")} bind:value={draft.jira.baseUrl} oninput={() => change()} />
            <span class="hint">{t("conn.urlHint")}</span>
          </div>
          <div class="field">
            <span class="label-text">{t("conn.method")}</span>
            {#each methods as [id, label] (id)}
              <label class="radio">
                <input type="radio" name="auth" value={id} bind:group={draft.jira.authMethod} onchange={() => change()} />
                <span>{t(label)}</span>
              </label>
            {/each}
            <span class="hint">{t(methodHelp[draft.jira.authMethod])}</span>
          </div>
          {#if draft.jira.authMethod === "password"}
            <div class="two">
              <div class="field"><label for="u">{t("conn.username")}</label><input id="u" type="text" bind:value={username} autocomplete="username" /></div>
              <div class="field"><label for="p">{t("conn.password")}</label><input id="p" type="password" bind:value={password} autocomplete="current-password" onkeydown={(e) => e.key === "Enter" && connect()} /></div>
            </div>
          {:else if draft.jira.authMethod === "personalAccessToken"}
            <div class="field"><label for="t">{t("conn.pat")}</label><input id="t" type="password" bind:value={token} onkeydown={(e) => e.key === "Enter" && connect()} /></div>
          {:else if draft.jira.authMethod === "apiToken"}
            <div class="two">
              <div class="field"><label for="e">{t("conn.email")}</label><input id="e" type="email" bind:value={email} /></div>
              <div class="field"><label for="t2">{t("conn.apiToken")}</label><input id="t2" type="password" bind:value={token} onkeydown={(e) => e.key === "Enter" && connect()} /></div>
            </div>
          {/if}
          {#if authError}<div class="error"><Icon name="warning" size={14} /> {authError}</div>{/if}
          <div class="row wrap">
            <button class="btn primary" type="button" onclick={connect} disabled={busy || !draft.jira.baseUrl}>
              {#if busy}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
              {t(auth.authenticated ? "conn.signInAgain" : "conn.signIn")}
            </button>
            {#if store.loginWindowOpen}
              <button class="btn" type="button" onclick={() => api.cancelBrowserLogin()}>{t("conn.closeLoginWindow")}</button>
              <span class="faint">{t("conn.loginInWindow")}</span>
            {/if}
            {#if auth.authenticated && !busy}
              <button class="btn" type="button" onclick={disconnect}>{t("conn.signOut")}</button>
            {/if}
          </div>
          {#if store.loginWindowOpen && store.loginProgress}
            <div class="progress"><span class="spin"><Icon name="refresh" size={13} /></span> {store.loginProgress}</div>
          {/if}
        </section>
        <section class="card block status">
          <h3>{t("status.title")}</h3>
          {#if auth.authenticated && auth.user}
            <div class="row"><span class="dot" style="background: var(--success)"></span><strong>{auth.user.displayName}</strong><span class="muted">{auth.user.email ?? ""}</span></div>
            {#if auth.server}
              <div class="muted">{auth.server.serverTitle ?? auth.server.baseUrl} · Jira {t(auth.server.deployment === "cloud" ? "status.cloud" : "status.dataCenter")} {auth.server.version}</div>
            {/if}
            <div class="muted">{t("status.via", { method: t(statusMethod[auth.method ?? ""] ?? "status.methodApiToken") })}</div>
          {:else}
            <div class="row"><span class="dot" style="background: var(--warning)"></span><span>{t("status.notSignedIn")}</span></div>
            {#if auth.message}<div class="muted">{auth.message}</div>{/if}
          {/if}
        </section>
        <section class="card block">
          <h3>{t("source.title")}</h3>
          {#each [["jiraWorklogs", "source.jira", "source.jiraHelp"], ["tempoCloud", "source.tempo", "source.tempoHelp"]] as [id, label, help] (id)}
            <label class="radio block-radio">
              <input type="radio" name="source" value={id} bind:group={draft.tempo.source} onchange={() => change(true)} />
              <span><strong>{t(label as Key)}</strong><span class="hint">{t(help as Key)}</span></span>
            </label>
          {/each}
          <p class="hint">{t("source.switchHint")}</p>
          {#if draft.tempo.source === "tempoCloud"}
            <div class="field">
              <label for="tbase">{t("source.apiBase")}</label>
              <input id="tbase" type="url" bind:value={draft.tempo.cloudApiBase} oninput={() => change()} />
            </div>
            <div class="field">
              <label for="ttoken">{t("source.token")} {draft.tempo.cloudTokenSet ? t("source.tokenStored") : ""}</label>
              <div class="row">
                <input id="ttoken" type="password" bind:value={tempoToken} placeholder={draft.tempo.cloudTokenSet ? "••••••••" : t("source.tokenPlaceholder")} onkeydown={(e) => e.key === "Enter" && tempoToken && saveTempoToken(false)} />
                <button class="btn primary" type="button" onclick={() => saveTempoToken(false)} disabled={tempoBusy || !tempoToken}>{t("source.checkSave")}</button>
                {#if draft.tempo.cloudTokenSet}<button class="btn danger" type="button" onclick={() => saveTempoToken(true)} disabled={tempoBusy}>{t("common.remove")}</button>{/if}
              </div>
              <span class="hint">{t("source.tokenHint")}</span>
            </div>
          {:else if lastOutcome && lastOutcome.fetched && lastOutcome.source === "jira" && otherAuthorsTotal > 0 && missingNames.length}
            <div class="error"><Icon name="warning" size={14} /> {t("source.jiraWarning", { names: missingNames.join(", "), n: otherAuthorsTotal, authors: lastOutcome.otherAuthors.map((o) => `${o.name || o.id}: ${o.count}`).join(", ") })}</div>
          {/if}
          {#if draft.tempo.source === "tempoCloud" && lastOutcome && lastOutcome.fetched && lastOutcome.source === "tempoCloud" && missingNames.length}
            <p class="hint">{t("source.tempoMissing", { names: missingNames.join(", ") })}</p>
          {/if}
        </section>
        <section class="card block">
          <h3>{t("compare.title")}</h3>
          <p class="hint">{t("compare.hint", { from: formatShortDate(store.range.from), to: formatShortDate(store.range.to), team: store.team?.name ?? t("compare.theTeam") })}</p>
          <div class="row wrap">
            <button class="btn" type="button" onclick={runCompare} disabled={comparing || !store.connected || !draft.tempo.cloudTokenSet || !store.memberIds.length}>
              {#if comparing}<span class="spin"><Icon name="refresh" size={14} /></span>{:else}<Icon name="layers" size={14} />{/if} {t("compare.start")}
            </button>
            {#if comparing}<span class="faint">{compareText}</span>{/if}
            {#if !draft.tempo.cloudTokenSet}<span class="faint">{t("compare.needToken")}</span>{/if}
          </div>
          {#if compareError}<div class="error"><Icon name="warning" size={14} /> {compareError}</div>{/if}
          {#if comparison}
            {@const diff = comparison.tempoSeconds - comparison.jiraSeconds}
            <p class="compare-summary">
              Jira <span class="mono">{formatHours(comparison.jiraSeconds, tf)}</span> {t("compare.onIssues", { n: comparison.jiraIssues })} ·
              Tempo <span class="mono">{formatHours(comparison.tempoSeconds, tf)}</span> {t("compare.onIssues", { n: comparison.tempoIssues })} ·
              {t("compare.diff")} <span class="mono" class:pos={diff === 0} class:neg={diff !== 0}>{formatSigned(diff, tf)}</span>
              {#if diff === 0}<span class="badge success">{t("compare.identical")}</span>{/if}
            </p>
            <table class="compare">
              <thead>
                <tr><th>{t("col.member")}</th><th class="num">Jira</th><th class="num">Tempo</th><th class="num">{t("compare.diff")}</th><th class="num">{t("compare.colDiffDays")}</th></tr>
              </thead>
              <tbody>
                {#each comparison.members as m (m.id)}
                  {@const d = m.tempoSeconds - m.jiraSeconds}
                  <tr class:clickable={m.differingDays.length > 0} onclick={() => (openDiff = openDiff === m.id ? null : m.id)}>
                    <td><span class="row"><Avatar name={store.memberName(m.id)} url={store.memberById(m.id)?.avatarUrl} color={store.memberColor(m.id)} size={22} />{store.memberName(m.id)}</span></td>
                    <td class="num mono">{formatHours(m.jiraSeconds, tf)}<span class="faint small"> ({m.jiraWorklogs})</span></td>
                    <td class="num mono">{formatHours(m.tempoSeconds, tf)}<span class="faint small"> ({m.tempoWorklogs})</span></td>
                    <td class="num mono" class:neg={d !== 0}>{formatSigned(d, tf)}</td>
                    <td class="num">{m.differingDays.length}{#if m.differingDays.length} <Icon name="chevronDown" size={12} />{/if}</td>
                  </tr>
                  {#if openDiff === m.id && m.differingDays.length}
                    <tr class="diff-row">
                      <td colspan="5">
                        <ul class="diff-days">
                          {#each m.differingDays as day (day.date)}
                            <li><span class="mono">{formatShortDate(day.date)}</span> Jira <span class="mono">{formatHours(day.jiraSeconds, tf)}</span> · Tempo <span class="mono">{formatHours(day.tempoSeconds, tf)}</span></li>
                          {/each}
                        </ul>
                      </td>
                    </tr>
                  {/if}
                {/each}
              </tbody>
            </table>
            {#if comparison.jiraOtherAuthors.length}
              <p class="hint">{t("compare.otherAuthors", { authors: comparison.jiraOtherAuthors.map((o) => `${o.name || o.id} (${o.count})`).join(", ") })}</p>
            {/if}
            {#if comparison.tempoUnresolvedIssues}
              <p class="hint">{t("compare.unresolved", { n: comparison.tempoUnresolvedIssues })}</p>
            {/if}
            <p class="hint">{t("compare.interpretation")}</p>
          {/if}
        </section>
      {:else if tab === "projects"}
        <section class="card block">
          <h3>{t("projects.title")}</h3>
          <p class="hint">{t("projects.hint")}</p>
          {#if selectedProjects.length}
            <div class="chips">
              {#each selectedProjects as p (p.key)}
                <span class="chip">
                  <input type="color" value={store.projectColor(p.key)} onchange={(e) => setColor(p.key, (e.target as HTMLInputElement).value)} title={t("common.color")} />
                  <span class="mono">{p.key}</span>
                  <span class="ellipsis">{p.name}</span>
                  <button type="button" onclick={() => toggleProject(p)} aria-label={t("common.remove")}><Icon name="x" size={12} /></button>
                </span>
              {/each}
            </div>
          {:else}
            <p class="muted">{t("projects.noRestriction")}</p>
          {/if}
          {#if !store.connected}
            <p class="muted">{t("projects.connectFirst")}</p>
          {:else}
            <div class="search row">
              <Icon name="search" size={15} />
              <input type="search" placeholder={t("projects.searchPlaceholder")} bind:value={projectQuery} oninput={onProjectQuery} />
              {#if projectsLoading}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
            </div>
            <ul class="list">
              {#each projectResults as p (p.key)}
                <li>
                  <label class="row">
                    <input type="checkbox" checked={selectedKeys.includes(p.key)} onchange={() => toggleProject(p)} />
                    <span class="mono key">{p.key}</span>
                    <span class="ellipsis grow">{p.name}</span>
                  </label>
                </li>
              {/each}
              {#if !projectResults.length && !projectsLoading}<li class="faint">{t("projects.none")}</li>{/if}
            </ul>
          {/if}
        </section>
      {:else if tab === "team"}
        <div class="teams-layout">
          <section class="card block team-list">
            <h3>{t("teams.title")}</h3>
            <ul class="team-items">
              {#each draft.teams as team (team.id)}
                <li>
                  <button type="button" class:on={selectedTeam?.id === team.id} onclick={() => selectTeam(team)}>
                    <span class="ellipsis grow">{team.name}</span>
                    <span class="badge">{team.members.length}</span>
                  </button>
                </li>
              {/each}
            </ul>
            <button class="btn sm" type="button" onclick={addTeam}><Icon name="plus" size={14} /> {t("teams.add")}</button>
          </section>
          {#if selectedTeam}
            <section class="card block team-detail">
              <div class="row head">
                <input class="team-name" type="text" value={selectedTeam.name} oninput={(e) => renameTeam(selectedTeam!, (e.target as HTMLInputElement).value)} placeholder={t("teams.name")} aria-label={t("teams.name")} />
                <span class="grow"></span>
                <button class="btn danger sm" type="button" onclick={() => removeTeam(selectedTeam!)}><Icon name="trash" size={14} /> {t("teams.delete")}</button>
              </div>
              <h3>{t("teams.members")}</h3>
              {#if selectedTeam.members.length}
                <table class="members">
                  <thead>
                    <tr>
                      <th>{t("col.member")}</th>
                      <th class="num" title={t("teams.colHoursTitle")}>{t("teams.colHours")}</th>
                      <th>{t("common.color")}</th>
                      <th></th>
                    </tr>
                  </thead>
                  <tbody>
                    {#each selectedTeam.members as m (m.id)}
                      <tr>
                        <td>
                          <span class="row">
                            <Avatar name={m.displayName} url={m.avatarUrl} color={m.color} size={28} />
                            <span class="texts"><span>{m.displayName}</span><span class="faint small">{m.email ?? m.id}</span></span>
                          </span>
                        </td>
                        <td class="num">
                          <input class="hours" type="number" min="0" max="80" step="0.5" value={m.weeklyHours ?? ""} placeholder={String(patternHours)} onchange={(e) => setWeeklyHours(m, (e.target as HTMLInputElement).value)} />
                        </td>
                        <td><input type="color" value={m.color ?? "#64748b"} onchange={(e) => setMemberColor(m, (e.target as HTMLInputElement).value)} title={t("teams.colorTitle")} /></td>
                        <td class="num"><button class="btn ghost sm icon" type="button" title={t("teams.removeMember")} onclick={() => removeMember(m)}><Icon name="x" size={14} /></button></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {:else}
                <p class="muted">{t("teams.noMembers")}</p>
              {/if}
              <h3 class="add-title">{t("teams.addMember")}</h3>
              {#if !store.connected}
                <p class="muted">{t("teams.connectFirst")}</p>
              {:else}
                <div class="row wrap">
                  <div class="search row grow">
                    <Icon name="search" size={15} />
                    <input type="search" placeholder={t(auth.server?.deployment === "cloud" ? "teams.searchCloud" : "teams.searchDc")} bind:value={userQuery} oninput={onUserQuery} onfocus={() => !userSearchDone && searchUsers()} />
                    {#if usersLoading}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
                  </div>
                  {#if auth.user && !memberInTeam(auth.user.id)}
                    <button class="btn sm" type="button" onclick={addMe}><Icon name="user" size={14} /> {t("teams.addMe")}</button>
                  {/if}
                </div>
                <ul class="list">
                  {#each userResults.filter((u) => !memberInTeam(u.id)) as u (u.id)}
                    <li>
                      <button type="button" class="user-row" onclick={() => addMember(u)}>
                        <Avatar name={u.displayName} url={u.avatarUrl} size={24} />
                        <span class="ellipsis">{u.displayName}</span>
                        <span class="faint ellipsis grow">{u.email ?? u.id}</span>
                        <Icon name="plus" size={14} />
                      </button>
                    </li>
                  {/each}
                  {#if userSearchDone && !usersLoading && !userResults.filter((u) => !memberInTeam(u.id)).length}
                    <li class="faint">{t(userQuery.trim() ? "teams.noUsers" : "teams.typeName")}</li>
                  {/if}
                </ul>
              {/if}
              <p class="hint">{t("teams.idHint")}</p>
            </section>
          {:else}
            <section class="card block team-detail">
              <p class="muted">{t("teams.noTeam")}</p>
            </section>
          {/if}
        </div>
      {:else if tab === "display"}
        <section class="card block">
          <h3>{t("display.targetTitle")}</h3>
          <p class="hint">{t("display.targetHint")}</p>
          <div class="weekdays">
            {#each weekdays as wd, i (wd)}
              <div class="field">
                <label for="wd{i}">{wd}</label>
                <input id="wd{i}" type="number" min="0" max="24" step="0.5" bind:value={draft.work.hoursPerWeekday[i]} onchange={() => change()} />
              </div>
            {/each}
          </div>
          <label class="check"><input type="checkbox" bind:checked={draft.work.showTargets} onchange={() => change()} /><span>{t("display.showTargets")}</span></label>
        </section>
        <section class="card block">
          <h3>{t("display.title")}</h3>
          <div class="two">
            <div class="field">
              <label for="language">{t("display.language")}</label>
              <select id="language" bind:value={draft.behavior.language} onchange={() => change(true)}>
                <option value="system">{t("lang.system")}</option>
                <option value="de">{t("lang.de")}</option>
                <option value="en">{t("lang.en")}</option>
              </select>
            </div>
            <div class="field">
              <label for="theme">{t("display.theme")}</label>
              <select id="theme" bind:value={draft.behavior.theme} onchange={() => change()}>
                <option value="system">{t("theme.system")}</option>
                <option value="light">{t("theme.light")}</option>
                <option value="dark">{t("theme.dark")}</option>
              </select>
            </div>
            <div class="field">
              <label for="fdow">{t("display.firstDay")}</label>
              <select id="fdow" bind:value={draft.work.firstDayOfWeek} onchange={() => change()}>
                <option value={0}>{t("display.monday")}</option>
                <option value={6}>{t("display.sunday")}</option>
              </select>
            </div>
            <div class="field">
              <label for="tfmt">{t("display.timeFormat")}</label>
              <select id="tfmt" bind:value={draft.work.timeFormat} onchange={() => change()}>
                <option value="hoursMinutes">{t("display.hoursMinutes")}</option>
                <option value="decimal">{t("display.decimal")}</option>
              </select>
            </div>
          </div>
        </section>
        <section class="card block">
          <h3>{t("refresh.title")}</h3>
          <label class="check"><input type="checkbox" bind:checked={draft.refresh.auto} onchange={() => change()} /><span>{t("refresh.auto")} <span class="hint">{t("refresh.autoHint")}</span></span></label>
          <div class="field">
            <label for="interval">{t("refresh.interval")}</label>
            <input id="interval" type="number" min="1" max="720" bind:value={draft.refresh.intervalMinutes} onchange={() => change()} />
          </div>
        </section>
      {:else if tab === "info"}
        <section class="card block">
          <h3>YaTTAT</h3>
          {#if info}
            <dl>
              <dt>{t("info.version")}</dt><dd>{info.version} ({info.platform})</dd>
              <dt>{t("info.settings")}</dt><dd class="mono small">{info.configDir}</dd>
              <dt>{t("info.cacheLogs")}</dt><dd class="mono small">{info.dataDir}</dd>
              <dt>{t("info.credentials")}</dt>
              <dd>
                {#if info.secretBackend === "keyring"}
                  {t("info.keyring")}
                {:else}
                  <span class="badge warning">{t("info.noKeyring")}</span> {t("info.keyFile")}
                {/if}
              </dd>
            </dl>
          {/if}
        </section>
        <section class="card block">
          <h3>{t("cache.title")}</h3>
          <p class="hint">{t("cache.hint")}</p>
          {#if info}
            <dl>
              <dt>{t("cache.worklogs")}</dt><dd>{info.cache.worklogs}</dd>
              <dt>{t("cache.issues")}</dt><dd>{info.cache.issues}</dd>
              <dt>{t("cache.coveredDays")}</dt><dd>{info.cache.coverageDays} {t("cache.memberDay")}</dd>
              <dt>{t("cache.size")}</dt><dd>{formatBytes(info.cache.databaseBytes)}</dd>
              <dt>{t("cache.lastFetch")}</dt><dd>{store.fetchStatus.lastSuccess ? formatDateTime(store.fetchStatus.lastSuccess) : "–"}</dd>
              {#if lastOutcome && lastOutcome.fetched}
                <dt>{t("cache.source")}</dt><dd>{t(lastOutcome.source === "tempoCloud" ? "cache.sourceTempo" : "cache.sourceJira")}</dd>
                <dt>{t("cache.result")}</dt><dd>{t("cache.resultText", { w: lastOutcome.worklogs, a: lastOutcome.authors - lastOutcome.authorsWithoutWorklogs.length, b: lastOutcome.authors, i: lastOutcome.issues, from: formatShortDate(lastOutcome.from), to: formatShortDate(lastOutcome.to) })}{lastOutcome.removed ? t("cache.removed", { n: lastOutcome.removed }) : ""}</dd>
                {#if lastOutcome.otherAuthors.length}
                  <dt>{t("cache.otherAuthors")}</dt><dd>{lastOutcome.otherAuthors.map((o) => `${o.name || o.id} (${o.count})`).join(", ")}<span class="hint"> {t("cache.otherAuthorsHint")}</span></dd>
                {/if}
                {#if lastOutcome.unresolvedIssues}
                  <dt>{t("cache.unreadable")}</dt><dd>{t("cache.unreadableText", { n: lastOutcome.unresolvedIssues })}</dd>
                {/if}
              {/if}
            </dl>
          {/if}
          <div class="row" style="margin-top:10px">
            <button class="btn danger" type="button" onclick={clearCache}><Icon name="trash" size={14} /> {t("cache.clear")}</button>
          </div>
        </section>
      {/if}
    </div>
  </div>
</div>

<style>
  .small {
    font-size: 12px;
  }
  .layout {
    display: flex;
    flex: 1;
    min-height: 0;
  }
  .tabs {
    width: 190px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 12px 0 20px;
  }
  .tabs button {
    text-align: left;
    border: none;
    background: transparent;
    padding: 7px 10px;
    border-radius: 7px;
    cursor: pointer;
    color: var(--text-2);
  }
  .tabs button:hover {
    background: var(--surface-2);
  }
  .tabs button.on {
    background: var(--surface);
    color: var(--accent);
    box-shadow: var(--shadow);
    font-weight: 600;
  }
  .pane {
    flex: 1;
    overflow: auto;
    padding: 0 20px 20px 8px;
    min-width: 0;
  }
  .block {
    padding: 16px 18px;
    margin-bottom: 12px;
    max-width: 860px;
  }
  .block h3 {
    margin: 0 0 10px;
    font-size: 14px;
  }
  .block p.hint,
  .hint {
    font-size: 12px;
    color: var(--text-3);
    margin: 0 0 10px;
  }
  .label-text {
    font-size: 12.5px;
    color: var(--text-2);
    font-weight: 500;
  }
  .radio {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    padding: 3px 0;
    cursor: pointer;
  }
  .block-radio {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .block-radio .hint {
    display: block;
    margin: 2px 0 0;
  }
  .two {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .weekdays {
    display: grid;
    grid-template-columns: repeat(7, 1fr);
    gap: 8px;
  }
  .weekdays .field {
    margin-bottom: 8px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--text-2);
    margin-top: 10px;
    padding: 8px 10px;
    background: var(--surface-2);
    border-radius: var(--radius-sm);
  }
  .error {
    color: var(--danger);
    display: flex;
    gap: 6px;
    align-items: center;
    margin: 4px 0 10px;
    font-size: 12.5px;
    user-select: text;
  }
  .wrap {
    flex-wrap: wrap;
  }
  .status .row {
    margin-bottom: 4px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 10px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12.5px;
    max-width: 320px;
  }
  .chip input[type="color"] {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    border: none;
  }
  .chip button {
    border: none;
    background: transparent;
    cursor: pointer;
    color: var(--text-3);
    display: inline-flex;
    padding: 2px;
  }
  .chip button:hover {
    color: var(--danger);
  }
  .search {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 8px;
    color: var(--text-3);
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
  .list {
    list-style: none;
    margin: 8px 0 0;
    padding: 0;
    max-height: 280px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .list li {
    padding: 6px 10px;
    border-bottom: 1px solid var(--border);
  }
  .list li:last-child {
    border-bottom: none;
  }
  .list label {
    cursor: pointer;
  }
  .list .key {
    color: var(--text-2);
    min-width: 56px;
  }
  .user-row {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    text-align: left;
    padding: 2px 0;
  }
  .user-row:hover {
    color: var(--accent);
  }
  .teams-layout {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    max-width: 860px;
  }
  .team-list {
    width: 220px;
    flex-shrink: 0;
  }
  .team-detail {
    flex: 1;
    min-width: 0;
  }
  .team-items {
    list-style: none;
    margin: 0 0 10px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .team-items button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    border: none;
    background: transparent;
    padding: 7px 10px;
    border-radius: 7px;
    cursor: pointer;
    color: var(--text-2);
    text-align: left;
  }
  .team-items button:hover {
    background: var(--surface-2);
  }
  .team-items button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .head {
    margin-bottom: 12px;
  }
  .team-name {
    font-weight: 600;
    font-size: 15px;
    min-width: 240px;
  }
  .add-title {
    margin-top: 16px !important;
  }
  table.members {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  table.members th {
    text-align: left;
    color: var(--text-2);
    font-weight: 500;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  table.members td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .texts {
    display: flex;
    flex-direction: column;
    line-height: 1.25;
  }
  .compare-summary {
    margin: 10px 0 8px;
  }
  .compare-summary .pos {
    color: var(--success);
  }
  .neg {
    color: var(--danger);
  }
  table.compare {
    width: 100%;
    border-collapse: collapse;
    font-size: 12.5px;
  }
  table.compare th {
    text-align: left;
    color: var(--text-2);
    font-weight: 500;
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  table.compare td {
    padding: 6px 8px;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  table.compare tr.clickable {
    cursor: pointer;
  }
  table.compare tr.clickable:hover td {
    background: var(--surface-2);
  }
  .diff-row td {
    background: var(--surface-2);
  }
  .diff-days {
    list-style: none;
    margin: 0;
    padding: 2px 0 2px 30px;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 2px 16px;
    font-size: 12px;
  }
  .num {
    text-align: right;
  }
  .hours {
    width: 80px;
    text-align: right;
  }
  dl {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 6px 12px;
    margin: 0;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
    user-select: text;
  }
</style>
