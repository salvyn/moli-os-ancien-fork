<script>
  import { onMount } from 'svelte';
  import { asHuman, CANCELLED, hub, note } from '../lib/home.svelte.js';
  import { request } from '../../lib/api.js';
  import { t } from '../../lib/i18n.svelte.js';
  import Icon from './Icon.svelte';

  /** The rooms of the house: which ones an agent never acts in without a
   *  person's approval, and the means to add, merge and rename them. Reading
   *  is free; opening the editor and every change want the house's code, and
   *  there is no switch to take this card out of that protection. A room
   *  exists through its devices (their label, else their origin) or because
   *  it was added here (`added`, kept by Moli with no device yet). */
  let protectedRooms = $state([]);
  let added = $state([]);
  let known = $state([]);
  let editing = $state(false);
  let draft = $state([]);
  let extra = $state('');
  let busy = $state(false);

  function take(body) {
    protectedRooms = body.protected_rooms ?? protectedRooms;
    added = body.added_rooms ?? added;
    known = body.known_rooms ?? known;
  }

  async function load() {
    try {
      take(await request('GET', '/api/guard/rooms'));
    } catch {
      /* next time */
    }
  }

  onMount(load);

  const clean = (s) => s.trim().replace(/\s+/g, ' ');
  const norm = (s) =>
    s
      .normalize('NFD')
      .replace(/\p{M}/gu, '')
      .trim()
      .replace(/\s+/g, ' ')
      .toLowerCase();
  const same = (a, b) => norm(a) === norm(b);
  const rawRoom = (d) => d.label?.room || d.native_room;
  /** Devices in a room, by the room they are filed in (labels first). */
  const devicesIn = (room) =>
    Object.values(hub.devices).filter((d) => {
      const raw = rawRoom(d);
      return raw && same(raw, room);
    });
  const count = (room) => devicesIn(room).length;
  const isOn = (room) => draft.some((d) => same(d, room));
  const wasProtected = (room) => protectedRooms.some((p) => same(p, room));
  const dedupe = (list) => list.filter((r, i) => list.findIndex((o) => same(o, r)) === i);

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

  /** Creates the room at once (kept by Moli), unchecked: protecting it is
   *  the checkbox, then « Enregistrer ». */
  async function addRoom() {
    const name = clean(extra);
    if (!name) return;
    if (choices.some((c) => same(c.name, name))) {
      note(t('systeme.pieces.existe_deja', { nom: name }), 'error');
      return;
    }
    busy = true;
    try {
      const body = await asHuman(t('systeme.pieces.ajout_label', { nom: name }), () =>
        request('PUT', '/api/guard/rooms', { added_rooms: [...added, name] }),
      );
      take({ added_rooms: body.added_rooms, known_rooms: body.known_rooms });
      extra = '';
      note(t('systeme.pieces.ajoutee', { nom: name }));
    } catch (err) {
      if (err.message !== CANCELLED) note(err.message, 'error');
    } finally {
      busy = false;
    }
  }

  /** Moves every device of `from` into `to` (a room « removed » by merging,
   *  or renamed). Never leaves a moved device unprotected: the target is
   *  protected first, the labels move, and only then the old name is dropped.
   *  The unsaved ticks follow the room. */
  async function moveRoom(from, to, label, rename) {
    const ids = devicesIn(from).map((d) => d.id);
    await asHuman(label, async () => {
      const guarded = wasProtected(from);
      const needsTarget = guarded && !wasProtected(to);
      if (needsTarget) {
        await request('PUT', '/api/guard/rooms', { protected_rooms: [...protectedRooms, to] });
      }
      for (const id of ids) {
        await request('PUT', `/api/labels/${encodeURIComponent(id)}`, { room: to });
      }
      const wasAdded = added.some((a) => same(a, from));
      if (guarded || wasAdded) {
        const rest = added.filter((a) => !same(a, from));
        const body = await request('PUT', '/api/guard/rooms', {
          ...(wasAdded ? { added_rooms: rename ? [...rest, to] : rest } : {}),
          ...(guarded
            ? { protected_rooms: [...protectedRooms.filter((p) => !same(p, from)), ...(needsTarget ? [to] : [])] }
            : {}),
        });
        take(body);
      }
    });
    draft = dedupe(draft.map((d) => (same(d, from) ? to : d)));
    await load();
    return ids.length;
  }

  // ---- merge: a room « deleted » by moving its devices into another ----
  let merging = $state(false);
  let from = $state('');
  let into = $state('');
  const moving = $derived(from ? count(from) : 0);

  function openMerge() {
    from = '';
    into = '';
    merging = true;
  }

  async function merge() {
    busy = true;
    try {
      const n = await moveRoom(from, into, t('systeme.pieces.fusion_label', { de: from, vers: into }), false);
      merging = false;
      note(t('systeme.pieces.fusion_faite', { count: n, vers: into }));
    } catch (err) {
      if (err.message !== CANCELLED) note(err.message, 'error');
      await load();
    } finally {
      busy = false;
    }
  }

  // ---- rename: a modal, the room then its new name ----
  let dialog;
  let renaming = $state('');
  let newName = $state('');
  const taken = $derived(
    !!clean(newName) && choices.some((c) => !same(c.name, renaming) && same(c.name, newName)),
  );
  const renameReady = $derived(!!renaming && !!clean(newName) && !taken && clean(newName) !== renaming);

  function openRename() {
    renaming = '';
    newName = '';
    dialog.showModal();
    dialog.focus();
  }

  function pickRename() {
    newName = renaming;
  }

  async function rename() {
    const to = clean(newName);
    const de = renaming;
    busy = true;
    // The code sheet sits under a modal: close it first, back if refused.
    dialog.close();
    try {
      await moveRoom(de, to, t('systeme.pieces.renommer_label', { de, vers: to }), true);
      note(t('systeme.pieces.renommee', { de, vers: to }));
    } catch (err) {
      if (err.message !== CANCELLED) note(err.message, 'error');
      await load();
      dialog.showModal();
    } finally {
      busy = false;
    }
  }

  async function save() {
    busy = true;
    try {
      const body = await asHuman(t('systeme.pieces.enregistrer_label'), () =>
        request('PUT', '/api/guard/rooms', { protected_rooms: draft }),
      );
      take(body);
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
  {#if !merging}
    <p class="muted aide">{editing ? t('systeme.pieces.aide_edition') : t('systeme.pieces.aide')}</p>
  {/if}

  {#if merging}
    <p class="muted">{t('systeme.pieces.fusion_aide')}</p>
    <div class="merge">
      <label class="pick">
        <span>{t('systeme.pieces.fusion_de')}</span>
        <select bind:value={from}>
          <option value="">—</option>
          {#each known as k (k)}<option value={k}>{k} ({count(k)})</option>{/each}
        </select>
      </label>
      <label class="pick">
        <span>{t('systeme.pieces.fusion_vers')}</span>
        <select bind:value={into}>
          <option value="">—</option>
          {#each known.filter((k) => !from || !same(k, from)) as k (k)}<option value={k}>{k} ({count(k)})</option>{/each}
        </select>
      </label>
    </div>
    {#if from && into}
      <p class="muted self">{t('systeme.pieces.fusion_resume', { count: moving, de: from, vers: into })}{wasProtected(from) ? ` ${t('systeme.pieces.fusion_protegee', { de: from, vers: into })}` : ''}</p>
    {/if}
    <div class="actions">
      <button class="ghost" disabled={busy} onclick={() => (merging = false)}>{t('systeme.pieces.annuler')}</button>
      <button class="primary" disabled={busy || !from || !into} onclick={merge}>{t('systeme.pieces.fusionner')}</button>
    </div>
  {:else if editing}
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
        addRoom();
      }}
    >
      <input type="text" bind:value={extra} maxlength="80" placeholder={t('systeme.pieces.autre')} aria-label={t('systeme.pieces.autre')} />
      <button type="submit" class="ghost" disabled={busy || !clean(extra)}>{t('systeme.pieces.ajouter')}</button>
    </form>
    <div class="tools">
      <button class="ghost" disabled={busy} onclick={openRename}>{t('systeme.pieces.renommer')}</button>
      <button class="ghost" disabled={busy} onclick={openMerge}>{t('systeme.pieces.fusionner')}</button>
    </div>
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

<dialog
  bind:this={dialog}
  onclick={(e) => e.target === dialog && dialog.close()}
  aria-labelledby="renommer-titre"
  tabindex="-1"
>
  <h3 id="renommer-titre">{t('systeme.pieces.renommer')}</h3>
  <p class="muted aide">{t('systeme.pieces.renommer_aide')}</p>
  <form
    onsubmit={(e) => {
      e.preventDefault();
      if (renameReady) rename();
    }}
  >
    <label class="pick">
      <span>{t('systeme.pieces.renommer_piece')}</span>
      <select bind:value={renaming} onchange={pickRename}>
        <option value="">—</option>
        {#each known as k (k)}<option value={k}>{k} ({count(k)})</option>{/each}
      </select>
    </label>
    <label class="pick">
      <span>{t('systeme.pieces.renommer_nouveau')}</span>
      <input type="text" bind:value={newName} maxlength="80" disabled={!renaming} />
    </label>
    <p class="muted self" role="alert">{taken ? t('systeme.pieces.renommer_pris') : ''}</p>
    <div class="actions">
      <button type="button" class="ghost" onclick={() => dialog.close()}>{t('systeme.pieces.annuler')}</button>
      <button type="submit" class="primary" disabled={busy || !renameReady}>{t('systeme.pieces.renommer_valider')}</button>
    </div>
  </form>
</dialog>

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

  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 12px;
  }

  dialog {
    border: 0;
    padding: 18px;
    width: min(440px, 94vw);
    max-height: 90dvh;
    overflow-y: auto;
    border-radius: var(--r-xl);
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 30px 80px rgb(0 0 0 / 40%);
  }

  dialog:focus {
    outline: none;
  }

  dialog::backdrop {
    background: rgb(4 6 12 / 60%);
    backdrop-filter: blur(4px);
  }

  dialog h3 {
    font-size: 18px;
    font-weight: 750;
    margin-bottom: 4px;
  }

  dialog form {
    display: grid;
    gap: 10px;
  }

  .pick input {
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--line, rgba(128, 128, 128, 0.35));
    border-radius: 10px;
    background: var(--surface);
    color: var(--ink);
    font: inherit;
  }

  .merge {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px 20px;
    margin-bottom: 12px;
  }

  .pick {
    flex-direction: column;
    align-items: stretch;
    gap: 4px;
    cursor: default;
  }

  .pick select {
    min-height: 40px;
    padding: 0 10px;
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
