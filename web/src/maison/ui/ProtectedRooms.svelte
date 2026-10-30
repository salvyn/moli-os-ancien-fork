<script>
  import { onMount } from 'svelte';
  import { asHuman, CANCELLED, hub, note } from '../lib/home.svelte.js';
  import { request } from '../../lib/api.js';
  import { t } from '../../lib/i18n.svelte.js';
  import Icon from './Icon.svelte';

  /** The rooms an agent never acts in without a person's approval. Reading
   *  is free; opening the editor and saving both want the house's code, and
   *  there is no switch to take this card out of that protection. */
  let protectedRooms = $state([]);
  let known = $state([]);
  let editing = $state(false);
  let draft = $state([]);
  let extra = $state('');
  let busy = $state(false);

  async function load() {
    try {
      const body = await request('GET', '/api/guard/rooms');
      protectedRooms = body.protected_rooms ?? [];
      known = body.known_rooms ?? [];
    } catch {
      /* next time */
    }
  }

  onMount(load);

  const norm = (s) =>
    s
      .normalize('NFD')
      .replace(/\p{M}/gu, '')
      .trim()
      .replace(/\s+/g, ' ')
      .toLowerCase();
  const same = (a, b) => norm(a) === norm(b);
  /** Devices in a room, by the room they are filed in (labels first). */
  const count = (room) =>
    Object.values(hub.devices).filter((d) => {
      const raw = d.label?.room || d.native_room;
      return raw && same(raw, room);
    }).length;
  const isOn = (room) => draft.some((d) => same(d, room));

  /** Every room worth a line: those of the devices, then protected ones
   *  that match no device (renamed? still kept, and said so). */
  const choices = $derived([
    ...known.map((name) => ({ name, lost: false })),
    ...draft.filter((d) => !known.some((k) => same(k, d))).map((name) => ({ name, lost: true })),
  ]);

  async function edit() {
    try {
      // The code first: the editor itself is behind it.
      await asHuman(t('systeme.pieces.ouvrir_label'), async () => true);
    } catch (err) {
      if (err.message !== CANCELLED) note(err.message, 'error');
      return;
    }
    await load();
    draft = [...protectedRooms];
    extra = '';
    editing = true;
  }

  function flip(room) {
    draft = isOn(room) ? draft.filter((d) => !same(d, room)) : [...draft, room];
  }

  function addExtra() {
    const name = extra.trim().replace(/\s+/g, ' ');
    if (name && !isOn(name)) draft = [...draft, name];
    extra = '';
  }

  async function save() {
    addExtra();
    busy = true;
    try {
      const body = await asHuman(t('systeme.pieces.enregistrer_label'), () =>
        request('PUT', '/api/guard/rooms', { protected_rooms: draft }),
      );
      protectedRooms = body.protected_rooms ?? [];
      known = body.known_rooms ?? known;
      editing = false;
      note(t('systeme.pieces.enregistre'));
    } catch (err) {
      if (err.message !== CANCELLED) note(err.message, 'error');
    } finally {
      busy = false;
    }
  }
</script>

<section class="card rooms">
  <div class="card-head">
    <h2><Icon name="shield" size={18} />{t('systeme.pieces.titre')}</h2>
    {#if !editing}
      <button class="primary" onclick={edit}><Icon name="lock" size={15} />{t('systeme.pieces.modifier')}</button>
    {/if}
  </div>
  <p class="muted aide">{t('systeme.pieces.aide')}</p>

  {#if editing}
    <ul class="choices">
      {#each choices as c (c.name)}
        <li>
          <label>
            <input type="checkbox" checked={isOn(c.name)} onchange={() => flip(c.name)} />
            <span>{c.name} ({count(c.name)})</span>
            {#if c.lost}<small class="muted">{t('systeme.pieces.sans_appareil')}</small>{/if}
          </label>
        </li>
      {/each}
    </ul>
    <form
      class="add"
      onsubmit={(e) => {
        e.preventDefault();
        addExtra();
      }}
    >
      <input type="text" bind:value={extra} maxlength="80" placeholder={t('systeme.pieces.autre')} aria-label={t('systeme.pieces.autre')} />
      <button type="submit" class="ghost" disabled={!extra.trim()}>{t('systeme.pieces.ajouter')}</button>
    </form>
    <p class="muted self"><Icon name="lock" size={14} />{t('systeme.pieces.toujours_protege')}</p>
    <div class="actions">
      <button class="ghost" disabled={busy} onclick={() => (editing = false)}>{t('systeme.pieces.annuler')}</button>
      <button class="primary" disabled={busy} onclick={save}>{t('systeme.pieces.enregistrer')}</button>
    </div>
  {:else if protectedRooms.length}
    <div class="chips">
      {#each protectedRooms as room (room)}<span class="chip">{room} ({count(room)})</span>{/each}
    </div>
  {:else}
    <p class="muted">{t('systeme.pieces.aucune')}</p>
  {/if}
</section>

<style>
  .card-head {
    margin-bottom: 6px;
  }

  .aide {
    font-size: 13.5px;
    margin-bottom: 12px;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .choices {
    list-style: none;
    margin: 0 0 12px;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 4px 20px;
  }

  label {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 40px;
    cursor: pointer;
    font-size: 14.5px;
  }

  input[type='checkbox'] {
    width: 20px;
    height: 20px;
    flex: none;
  }

  .add {
    display: flex;
    gap: 8px;
    margin-bottom: 10px;
  }

  .add input {
    flex: 1;
    min-width: 0;
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--line, rgba(128, 128, 128, 0.35));
    border-radius: 10px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
  }

  .self {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    margin-bottom: 12px;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }

  .primary,
  .ghost {
    min-height: 40px;
    padding: 0 18px;
    border-radius: 999px;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .primary {
    border: none;
    background: var(--ink);
    color: var(--surface);
  }

  .ghost {
    border: 1px solid var(--line, rgba(128, 128, 128, 0.35));
    background: transparent;
    color: var(--ink);
  }

  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
