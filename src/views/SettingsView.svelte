<script lang="ts">
  import { store } from "../lib/store.svelte";
  import { api, type AppInfo, type AuthMethod, type JiraUser, type Member, type Project, type Settings, type Team } from "../lib/api";
  import { formatDateTime } from "../lib/format";
  import Icon from "../components/Icon.svelte";
  import Avatar from "../components/Avatar.svelte";

  type Tab = "connection" | "projects" | "team" | "display" | "info";
  // Browser mock only: ?tab=team opens a settings tab directly (used for screenshots).
  const requestedTab = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window) ? new URLSearchParams(location.search).get("tab") : null;
  let tab = $state<Tab>((["connection", "projects", "team", "display", "info"].includes(requestedTab ?? "") ? requestedTab : store.connected ? "team" : "connection") as Tab);
  const tabs: { id: Tab; label: string }[] = [
    { id: "connection", label: "Verbindung" },
    { id: "projects", label: "Projekte" },
    { id: "team", label: "Team & Mitglieder" },
    { id: "display", label: "Zeit & Darstellung" },
    { id: "info", label: "Info" },
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
  const methodHelp: Record<AuthMethod, string> = {
    browserSession: "Öffnet die Jira-Anmeldeseite in einem eigenen Fenster. SSO und ein zweiter Faktor funktionieren wie im Browser. Es wird nur die Sitzung gespeichert, kein Passwort.",
    password: "Für Jira Data Center ohne SSO. Benutzername und Passwort werden verschlüsselt lokal gespeichert.",
    personalAccessToken: "Für Jira Data Center: Personal Access Token aus dem Profil (Profil → Persönliche Zugangstoken).",
    apiToken: "Für Jira Cloud: E-Mail-Adresse und API-Token von id.atlassian.com.",
  };

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
      store.toast(`Angemeldet als ${store.auth.user?.displayName ?? "?"}`, "success");
      if (!store.members.length) tab = "team";
    } catch (e) {
      authError = String(e);
    } finally {
      busy = false;
    }
  }

  async function disconnect() {
    if (!(await store.confirm("Abmelden", "Die gespeicherte Jira-Sitzung wird gelöscht. Die bereits geladenen Zeiten bleiben lokal erhalten.", "Abmelden"))) return;
    await api.logout();
    store.auth = await api.getAuthStatus();
  }

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
    const team: Team = { id: newId("team"), name: `Team ${draft.teams.length + 1}`, members: [] };
    draft.teams = [...draft.teams, team];
    selectedTeamId = team.id;
    change();
  }
  async function removeTeam(t: Team) {
    if (!draft) return;
    if (!(await store.confirm("Team löschen", `Soll das Team „${t.name}“ mit ${t.members.length} Mitgliedern entfernt werden? Die geladenen Zeiten bleiben im lokalen Cache.`, "Löschen", true))) return;
    draft.teams = draft.teams.filter((x) => x.id !== t.id);
    selectedTeamId = draft.teams[0]?.id ?? null;
    if (store.teamId === t.id) store.teamId = selectedTeamId;
    change(true);
  }
  function renameTeam(t: Team, name: string) {
    t.name = name;
    change();
  }
  function selectTeam(t: Team) {
    selectedTeamId = t.id;
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
    if (!(await store.confirm("Mitglied entfernen", `${m.displayName} aus „${selectedTeam.name}“ entfernen?`, "Entfernen", true))) return;
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
    for (const t of draft?.teams ?? []) for (const x of t.members) if (x.id === m.id) x.color = color;
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
    if (!(await store.confirm("Cache leeren", "Alle lokal gespeicherten Tickets und Worklogs werden gelöscht und beim nächsten Anzeigen neu aus Jira geladen.", "Cache leeren", true))) return;
    try {
      const cache = await api.clearCache();
      if (info) info = { ...info, cache };
      store.toast("Cache geleert", "success");
    } catch (e) {
      store.toast(String(e), "error");
    }
  }
  function formatBytes(n: number): string {
    if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
    return `${(n / 1024 / 1024).toFixed(1).replace(".", ",")} MB`;
  }

  const weekdays = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];
</script>

<div class="view">
  <div class="view-header">
    <h1>Einstellungen</h1>
    <div class="grow"></div>
    {#if dirty}<span class="faint">Speichere …</span>{/if}
  </div>
  <div class="layout">
    <nav class="tabs">
      {#each tabs as t (t.id)}
        <button class:on={tab === t.id} type="button" onclick={() => (tab = t.id)}>{t.label}</button>
      {/each}
    </nav>
    <div class="pane">
      {#if !draft}
        <p class="faint">Lade …</p>
      {:else if tab === "connection"}
        <section class="card block">
          <h3>Jira-Verbindung</h3>
          <div class="field">
            <label for="jira-url">Jira-URL</label>
            <input id="jira-url" type="url" placeholder="https://jira.firma.de oder https://firma.atlassian.net" bind:value={draft.jira.baseUrl} oninput={() => change()} />
            <span class="hint">Jira Cloud: nur die Domain, z. B. https://firma.atlassian.net (ohne /jira). Data Center ggf. mit Kontextpfad, z. B. https://tools.firma.de/jira. Eingefügte Ticket- oder Board-Links werden automatisch gekürzt.</span>
          </div>
          <div class="field">
            <span class="label-text">Anmeldeart</span>
            {#each [["browserSession", "Browser-Anmeldung (SSO, 2FA)"], ["password", "Benutzername und Passwort"], ["personalAccessToken", "Personal Access Token (Data Center)"], ["apiToken", "E-Mail und API-Token (Cloud)"]] as [id, label] (id)}
              <label class="radio">
                <input type="radio" name="auth" value={id} bind:group={draft.jira.authMethod} onchange={() => change()} />
                <span>{label}</span>
              </label>
            {/each}
            <span class="hint">{methodHelp[draft.jira.authMethod]}</span>
          </div>
          {#if draft.jira.authMethod === "password"}
            <div class="two">
              <div class="field"><label for="u">Benutzername</label><input id="u" type="text" bind:value={username} autocomplete="username" /></div>
              <div class="field"><label for="p">Passwort</label><input id="p" type="password" bind:value={password} autocomplete="current-password" onkeydown={(e) => e.key === "Enter" && connect()} /></div>
            </div>
          {:else if draft.jira.authMethod === "personalAccessToken"}
            <div class="field"><label for="t">Personal Access Token</label><input id="t" type="password" bind:value={token} onkeydown={(e) => e.key === "Enter" && connect()} /></div>
          {:else if draft.jira.authMethod === "apiToken"}
            <div class="two">
              <div class="field"><label for="e">E-Mail</label><input id="e" type="email" bind:value={email} /></div>
              <div class="field"><label for="t2">API-Token</label><input id="t2" type="password" bind:value={token} onkeydown={(e) => e.key === "Enter" && connect()} /></div>
            </div>
          {/if}
          {#if authError}<div class="error"><Icon name="warning" size={14} /> {authError}</div>{/if}
          <div class="row wrap">
            <button class="btn primary" type="button" onclick={connect} disabled={busy || !draft.jira.baseUrl}>
              {#if busy}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
              {auth.authenticated ? "Erneut anmelden" : "Anmelden"}
            </button>
            {#if store.loginWindowOpen}
              <button class="btn" type="button" onclick={() => api.cancelBrowserLogin()}>Anmeldefenster schließen</button>
              <span class="faint">Bitte im geöffneten Fenster bei Jira anmelden …</span>
            {/if}
            {#if auth.authenticated && !busy}
              <button class="btn" type="button" onclick={disconnect}>Abmelden</button>
            {/if}
          </div>
          {#if store.loginWindowOpen && store.loginProgress}
            <div class="progress"><span class="spin"><Icon name="refresh" size={13} /></span> {store.loginProgress}</div>
          {/if}
        </section>
        <section class="card block status">
          <h3>Status</h3>
          {#if auth.authenticated && auth.user}
            <div class="row"><span class="dot" style="background: var(--success)"></span><strong>{auth.user.displayName}</strong><span class="muted">{auth.user.email ?? ""}</span></div>
            {#if auth.server}
              <div class="muted">{auth.server.serverTitle ?? auth.server.baseUrl} · Jira {auth.server.deployment === "cloud" ? "Cloud" : "Data Center"} {auth.server.version}</div>
            {/if}
            <div class="muted">Anmeldung über {auth.method === "cookies" ? "Browser-Sitzung" : auth.method === "basic" ? "Benutzername/Passwort" : auth.method === "bearer" ? "Personal Access Token" : "API-Token"}</div>
          {:else}
            <div class="row"><span class="dot" style="background: var(--warning)"></span><span>Nicht angemeldet</span></div>
            {#if auth.message}<div class="muted">{auth.message}</div>{/if}
          {/if}
        </section>
      {:else if tab === "projects"}
        <section class="card block">
          <h3>Projekte</h3>
          <p class="hint">
            Optional: Nur Zeiten auf Tickets dieser Projekte werden geladen und ausgewertet. Ohne Auswahl zählen alle Projekte,
            auf die die Teammitglieder Zeiten gebucht haben.
          </p>
          {#if selectedProjects.length}
            <div class="chips">
              {#each selectedProjects as p (p.key)}
                <span class="chip">
                  <input type="color" value={store.projectColor(p.key)} onchange={(e) => setColor(p.key, (e.target as HTMLInputElement).value)} title="Farbe" />
                  <span class="mono">{p.key}</span>
                  <span class="ellipsis">{p.name}</span>
                  <button type="button" onclick={() => toggleProject(p)} aria-label="Entfernen"><Icon name="x" size={12} /></button>
                </span>
              {/each}
            </div>
          {:else}
            <p class="muted">Keine Einschränkung: alle Projekte.</p>
          {/if}
          {#if !store.connected}
            <p class="muted">Zum Suchen zuerst mit Jira verbinden.</p>
          {:else}
            <div class="search row">
              <Icon name="search" size={15} />
              <input type="search" placeholder="Projekte suchen (Key oder Name) …" bind:value={projectQuery} oninput={onProjectQuery} />
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
              {#if !projectResults.length && !projectsLoading}<li class="faint">Keine Projekte gefunden.</li>{/if}
            </ul>
          {/if}
        </section>
      {:else if tab === "team"}
        <div class="teams-layout">
          <section class="card block team-list">
            <h3>Teams</h3>
            <ul class="team-items">
              {#each draft.teams as t (t.id)}
                <li>
                  <button type="button" class:on={selectedTeam?.id === t.id} onclick={() => selectTeam(t)}>
                    <span class="ellipsis grow">{t.name}</span>
                    <span class="badge">{t.members.length}</span>
                  </button>
                </li>
              {/each}
            </ul>
            <button class="btn sm" type="button" onclick={addTeam}><Icon name="plus" size={14} /> Team anlegen</button>
          </section>
          {#if selectedTeam}
            <section class="card block team-detail">
              <div class="row head">
                <input class="team-name" type="text" value={selectedTeam.name} oninput={(e) => renameTeam(selectedTeam!, (e.target as HTMLInputElement).value)} placeholder="Teamname" aria-label="Teamname" />
                <span class="grow"></span>
                <button class="btn danger sm" type="button" onclick={() => removeTeam(selectedTeam!)}><Icon name="trash" size={14} /> Team löschen</button>
              </div>
              <h3>Mitglieder</h3>
              {#if selectedTeam.members.length}
                <table class="members">
                  <thead>
                    <tr>
                      <th>Mitglied</th>
                      <th class="num" title="Vertragliche Wochenstunden für das Soll; leer = Vollzeit-Muster aus „Zeit & Darstellung“">Std./Woche</th>
                      <th>Farbe</th>
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
                        <td><input type="color" value={m.color ?? "#64748b"} onchange={(e) => setMemberColor(m, (e.target as HTMLInputElement).value)} title="Farbe in Diagrammen" /></td>
                        <td class="num"><button class="btn ghost sm icon" type="button" title="Aus dem Team entfernen" onclick={() => removeMember(m)}><Icon name="x" size={14} /></button></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              {:else}
                <p class="muted">Noch keine Mitglieder. Füge unten Jira-Benutzer hinzu.</p>
              {/if}
              <h3 class="add-title">Mitglied hinzufügen</h3>
              {#if !store.connected}
                <p class="muted">Zum Suchen von Jira-Benutzern zuerst mit Jira verbinden.</p>
              {:else}
                <div class="row wrap">
                  <div class="search row grow">
                    <Icon name="search" size={15} />
                    <input type="search" placeholder={auth.server?.deployment === "cloud" ? "Name oder E-Mail eingeben …" : "Name, Benutzername oder E-Mail (leer = alle) …"} bind:value={userQuery} oninput={onUserQuery} onfocus={() => !userSearchDone && searchUsers()} />
                    {#if usersLoading}<span class="spin"><Icon name="refresh" size={14} /></span>{/if}
                  </div>
                  {#if auth.user && !memberInTeam(auth.user.id)}
                    <button class="btn sm" type="button" onclick={addMe}><Icon name="user" size={14} /> Mich hinzufügen</button>
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
                    <li class="faint">{userQuery.trim() ? "Keine passenden Benutzer gefunden." : "Bitte einen Namen eingeben."}</li>
                  {/if}
                </ul>
              {/if}
              <p class="hint">
                Die Zeiten werden über den Jira-Benutzer abgerufen (Cloud: Account-ID, Data Center: Benutzername). Ein Mitglied kann in
                mehreren Teams sein; Farbe und Wochenstunden gelten dann überall.
              </p>
            </section>
          {:else}
            <section class="card block team-detail">
              <p class="muted">Lege ein Team an, um Mitglieder hinzuzufügen.</p>
            </section>
          {/if}
        </div>
      {:else if tab === "display"}
        <section class="card block">
          <h3>Soll-Arbeitszeit (Vollzeit)</h3>
          <p class="hint">Stunden pro Wochentag für ein Vollzeit-Mitglied. Mitglieder mit eingetragenen Wochenstunden werden anteilig skaliert.</p>
          <div class="weekdays">
            {#each weekdays as wd, i (wd)}
              <div class="field">
                <label for="wd{i}">{wd}</label>
                <input id="wd{i}" type="number" min="0" max="24" step="0.5" bind:value={draft.work.hoursPerWeekday[i]} onchange={() => change()} />
              </div>
            {/each}
          </div>
          <label class="check"><input type="checkbox" bind:checked={draft.work.showTargets} onchange={() => change()} /><span>Soll und Saldo in den Auswertungen anzeigen</span></label>
        </section>
        <section class="card block">
          <h3>Darstellung</h3>
          <div class="two">
            <div class="field">
              <label for="fdow">Wochenbeginn</label>
              <select id="fdow" bind:value={draft.work.firstDayOfWeek} onchange={() => change()}>
                <option value={0}>Montag</option>
                <option value={6}>Sonntag</option>
              </select>
            </div>
            <div class="field">
              <label for="tfmt">Zeitformat</label>
              <select id="tfmt" bind:value={draft.work.timeFormat} onchange={() => change()}>
                <option value="hoursMinutes">Stunden:Minuten (1:30)</option>
                <option value="decimal">Dezimal (1,50)</option>
              </select>
            </div>
            <div class="field">
              <label for="theme">Erscheinungsbild</label>
              <select id="theme" bind:value={draft.behavior.theme} onchange={() => change()}>
                <option value="system">System</option>
                <option value="light">Hell</option>
                <option value="dark">Dunkel</option>
              </select>
            </div>
          </div>
        </section>
        <section class="card block">
          <h3>Aktualisierung</h3>
          <label class="check"><input type="checkbox" bind:checked={draft.refresh.auto} onchange={() => change()} /><span>Angezeigten Zeitraum automatisch neu aus Jira laden <span class="hint">Nachträglich gebuchte oder geänderte Zeiten werden so übernommen. „Aktualisieren“ in der Kopfzeile lädt jederzeit sofort.</span></span></label>
          <div class="field">
            <label for="interval">Intervall (Minuten)</label>
            <input id="interval" type="number" min="1" max="720" bind:value={draft.refresh.intervalMinutes} onchange={() => change()} />
          </div>
        </section>
      {:else if tab === "info"}
        <section class="card block">
          <h3>YaTTAT</h3>
          {#if info}
            <dl>
              <dt>Version</dt><dd>{info.version} ({info.platform})</dd>
              <dt>Einstellungen</dt><dd class="mono small">{info.configDir}</dd>
              <dt>Cache & Logs</dt><dd class="mono small">{info.dataDir}</dd>
              <dt>Zugangsdaten</dt>
              <dd>
                {#if info.secretBackend === "keyring"}
                  Verschlüsselt; Schlüssel im Schlüsselbund des Betriebssystems.
                {:else}
                  <span class="badge warning">Kein Schlüsselbund verfügbar</span> Schlüssel liegt als Datei mit eingeschränkten Rechten im Konfigurationsordner.
                {/if}
              </dd>
            </dl>
          {/if}
        </section>
        <section class="card block">
          <h3>Lokaler Cache</h3>
          <p class="hint">Aus Jira geladene Tickets und Worklogs werden lokal gespeichert, damit Auswertungen sofort erscheinen. Bereits geladene Tage werden nicht erneut abgerufen, außer über „Aktualisieren“ oder die automatische Aktualisierung.</p>
          {#if info}
            <dl>
              <dt>Worklogs</dt><dd>{info.cache.worklogs}</dd>
              <dt>Tickets</dt><dd>{info.cache.issues}</dd>
              <dt>Abgedeckte Tage</dt><dd>{info.cache.coverageDays} (Mitglied × Tag)</dd>
              <dt>Größe</dt><dd>{formatBytes(info.cache.databaseBytes)}</dd>
              <dt>Letzter Abruf</dt><dd>{store.fetchStatus.lastSuccess ? formatDateTime(store.fetchStatus.lastSuccess) : "–"}</dd>
            </dl>
          {/if}
          <div class="row" style="margin-top:10px">
            <button class="btn danger" type="button" onclick={clearCache}><Icon name="trash" size={14} /> Cache leeren</button>
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
