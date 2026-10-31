<script lang="ts">
  // Confirmation dialog and toasts.
  import { store } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import { t } from "../lib/i18n.svelte";

  const confirm = $derived(store.confirmRequest);

  function answer(ok: boolean) {
    const req = store.confirmRequest;
    store.confirmRequest = null;
    req?.resolve(ok);
  }
</script>

{#if confirm}
  <div class="overlay">
    <div class="dialog" role="alertdialog" aria-modal="true">
      <h2>{confirm.title}</h2>
      <p>{confirm.message}</p>
      <div class="actions">
        <button class="btn" type="button" onclick={() => answer(false)}>{t("common.cancel")}</button>
        <button class="btn" class:danger={confirm.danger} class:primary={!confirm.danger} type="button" onclick={() => answer(true)}>{confirm.confirmLabel}</button>
      </div>
    </div>
  </div>
{/if}

<div class="toasts no-print">
  {#each store.toasts as toast (toast.id)}
    <div class="toast {toast.kind}">
      <span class="grow">{toast.message}</span>
      <button class="btn ghost sm icon" type="button" onclick={() => store.dismissToast(toast.id)} aria-label={t("common.close")}><Icon name="x" size={14} /></button>
    </div>
  {/each}
</div>

<style>
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 60;
    max-width: 440px;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-lg);
    border-left: 4px solid var(--accent);
    user-select: text;
  }
  .toast.success {
    border-left-color: var(--success);
  }
  .toast.error {
    border-left-color: var(--danger);
  }
</style>
