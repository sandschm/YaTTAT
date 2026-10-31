<script lang="ts">
  import { onMount } from "svelte";
  import { store, type View } from "./lib/store.svelte";
  import Icon from "./components/Icon.svelte";
  import TopBar from "./components/TopBar.svelte";
  import Dialogs from "./components/Dialogs.svelte";
  import TeamView from "./views/TeamView.svelte";
  import MemberView from "./views/MemberView.svelte";
  import TicketsView from "./views/TicketsView.svelte";
  import SettingsView from "./views/SettingsView.svelte";
  import { i18n, resolveLanguage, t, type Key } from "./lib/i18n.svelte";

  const nav: { id: View; label: Key; icon: string }[] = [
    { id: "team", label: "nav.team", icon: "users" },
    { id: "member", label: "nav.member", icon: "user" },
    { id: "tickets", label: "nav.tickets", icon: "ticket" },
    { id: "settings", label: "nav.settings", icon: "settings" },
  ];

  onMount(() => {
    store.init();
  });

  $effect(() => {
    i18n.lang = resolveLanguage(store.settings?.behavior.language);
    document.documentElement.lang = i18n.lang;
  });

  $effect(() => {
    const theme = store.settings?.behavior.theme ?? "system";
    const root = document.documentElement;
    if (theme === "system") root.removeAttribute("data-theme");
    else root.setAttribute("data-theme", theme);
  });

  // Keep the cache in sync with what is displayed: range or team changed -> fetch missing days.
  $effect(() => {
    const { from, to } = store.range;
    const ids = store.memberIds;
    const connected = store.connected;
    // the source and its token decide whether a fetch is possible at all
    const source = store.settings?.tempo.source;
    const tokenSet = store.settings?.tempo.cloudTokenSet;
    if (!store.ready || !connected || !ids.length) return;
    void from;
    void to;
    void source;
    void tokenSet;
    store.ensureData(false);
  });

  function onKey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key >= "1" && e.key <= "4") {
      store.view = nav[Number(e.key) - 1].id;
      e.preventDefault();
    }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="shell">
  <aside class="sidebar no-print">
    <div class="brand">
      <span class="logo"><Icon name="chart" size={18} /></span>
      <span>YaTTAT</span>
    </div>
    <nav>
      {#each nav as item (item.id)}
        <button class="nav-item" class:active={store.view === item.id} type="button" onclick={() => (store.view = item.id)}>
          <Icon name={item.icon} size={17} />
          <span>{t(item.label)}</span>
        </button>
      {/each}
    </nav>
    <div class="sidebar-foot">
      {#if store.connected}
        <div class="conn ok" title={store.auth.server?.baseUrl ?? ""}>
          <span class="dot" style="background: var(--success)"></span>
          <span class="ellipsis">{store.auth.user?.displayName ?? t("app.connected")}</span>
        </div>
      {:else}
        <button class="conn warn" type="button" onclick={() => (store.view = "settings")}>
          <span class="dot" style="background: var(--warning)"></span>
          <span class="ellipsis">{t("app.notConnected")}</span>
        </button>
      {/if}
    </div>
  </aside>
  <main class="main">
    {#if store.ready}
      {#if store.view !== "settings"}
        <TopBar />
      {/if}
      <div class="content">
        {#if store.view === "team"}
          <TeamView />
        {:else if store.view === "member"}
          <MemberView />
        {:else if store.view === "tickets"}
          <TicketsView />
        {:else}
          <SettingsView />
        {/if}
      </div>
    {:else}
      <div class="loading faint">{t("common.loading")}</div>
    {/if}
  </main>
</div>
<Dialogs />

<style>
  .shell {
    display: flex;
    height: 100%;
  }
  .sidebar {
    width: var(--sidebar-w);
    background: var(--surface-2);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 14px 10px;
    gap: 6px;
    flex-shrink: 0;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    font-size: 15px;
    padding: 4px 8px 14px;
    letter-spacing: 0.2px;
  }
  .logo {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px;
    border: none;
    background: transparent;
    border-radius: 8px;
    cursor: pointer;
    color: var(--text-2);
    text-align: left;
    font-weight: 500;
  }
  .nav-item:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--surface);
    color: var(--accent);
    box-shadow: var(--shadow);
  }
  .sidebar-foot {
    margin-top: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }
  .conn {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: var(--text-2);
    text-align: left;
    width: 100%;
  }
  .conn.warn {
    cursor: pointer;
    background: var(--warning-soft);
    color: var(--warning);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .content {
    flex: 1;
    min-height: 0;
  }
  .loading {
    padding: 40px;
    text-align: center;
  }
</style>
