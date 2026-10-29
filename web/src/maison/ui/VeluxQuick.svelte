<script>
  // The top of the Windows page: every blind or every roof window in one
  // tap (one order to the VELUX gateway, as the app's own buttons), the
  // house locked or not (the gateway's departure mode: nothing moves), and
  // the rain the gateway feels.
  import { hub, value, act, note, coverIcon, reachable } from '../lib/home.svelte.js';
  import { t } from '../../lib/i18n.svelte.js';
  import Icon from './Icon.svelte';

  const writable = (d, key) => d.points.some((p) => p.key === key && p.access?.write);
  const gateway = $derived(Object.values(hub.devices).find((d) => writable(d, 'departure')));
  const covers = $derived(
    gateway ? Object.values(hub.devices).filter((d) => d.instance === gateway.instance && d.points.some((p) => p.key === 'percent_control')) : [],
  );
  const windows = $derived(covers.filter((d) => coverIcon(d) === 'roof-window'));
  const blinds = $derived(covers.filter((d) => coverIcon(d) !== 'roof-window'));

  const locked = $derived(gateway ? value(gateway.id, 'departure') === true : false);
  const rain = $derived(gateway ? value(gateway.id, 'rain') : null);
  const raining = $derived(rain === true);

  /** How many of `list` are open (0 is closed, a blind all the way down),
   *  and whether one moves. */
  function tally(list) {
    const read = list.filter((d) => reachable(d.id) && typeof value(d.id, 'percent_control') === 'number');
    return {
      open: read.filter((d) => value(d.id, 'percent_control') > 0).length,
      // All the way (100 %): « all up » is said only when every one is.
      full: read.filter((d) => value(d.id, 'percent_control') >= 100).length,
      total: list.length,
      // Every window at the vent bar (10 %): « ventilation en cours ».
      venting: read.length > 0 && read.length === list.length && read.every((d) => value(d.id, 'percent_control') === 10),
      moving: list.some((d) => value(d.id, 'moving') === true),
    };
  }

  const rows = $derived(
    [
      {
        key: 'blinds',
        title: t('maison.fenetres.rapide.stores'),
        tally: tally(blinds),
        close: { label: t('maison.fenetres.rapide.fermer_stores'), icon: 'blinds-closed' },
        open: { label: t('maison.fenetres.rapide.ouvrir_stores'), icon: 'blinds-open' },
        said: (s) =>
          s.open === 0
            ? t('maison.fenetres.rapide.stores_tous_baisses')
            : s.full === s.total
              ? t('maison.fenetres.rapide.stores_tous_leves')
              : s.full === 0
                ? t('maison.fenetres.rapide.stores_leves_en_partie')
                : t('maison.fenetres.rapide.stores_leves_et_partiels', { n: s.full, count: s.total, part: s.open - s.full }),
      },
      {
        key: 'windows',
        title: t('maison.fenetres.rapide.fenetres'),
        tally: tally(windows),
        close: { label: t('maison.fenetres.rapide.fermer_fenetres'), icon: 'window-closed' },
        open: { label: t('maison.fenetres.rapide.ouvrir_fenetres'), icon: 'window-open', rainy: true },
        // Between opening and closing: every window at 10 %, the vent bar.
        air: { label: t('maison.fenetres.rapide.aerer_fenetres'), hint: t('maison.fenetres.rapide.aerer_aide'), icon: 'window-vent', rainy: true },
        said: (s) =>
          s.venting
            ? t('maison.fenetres.rapide.ventilation_en_cours')
            : s.open === 0 ? t('maison.fenetres.rapide.fenetres_toutes_fermees') : t('maison.fenetres.rapide.fenetres_ouvertes', { n: s.open, count: s.total }),
      },
    ].filter((r) => gateway && writable(gateway, r.key) && r.tally.total),
  );

  /** The order on its way: `${row}/${what}`, until the gateway answers. */
  let busy = $state(null);

  async function go(row, what, label) {
    if (busy) return;
    busy = `${row}/${what}`;
    const done = await act(`${gateway.id}/${row}`, what, label);
    busy = null;
    if (done === 'ok') note(t(what === 'stop' ? 'maison.fenetres.rapide.arrete' : 'maison.fenetres.rapide.envoye', { label }));
  }

  async function lock(on) {
    if (busy) return;
    busy = 'departure';
    const label = t(on ? 'maison.fenetres.rapide.label_verrouiller' : 'maison.fenetres.rapide.label_deverrouiller');
    const done = await act(`${gateway.id}/departure`, on, label);
    busy = null;
    if (done === 'ok') note(t(on ? 'maison.fenetres.rapide.verrouillee_note' : 'maison.fenetres.rapide.deverrouillee_note'));
  }
  // Unlocking is signed with the gateway's key, as the windows' orders:
  // without it (no window to move), the button says so.
  const canUnlock = $derived(!!gateway && writable(gateway, 'windows'));
</script>

{#if gateway}
  <div class="quick">
    {#if rows.length}
      <section class="card actions" aria-labelledby="velux-quick">
        <div class="card-head">
          <h2 id="velux-quick"><Icon name="hand" size={18} />{t('maison.fenetres.rapide.titre')}</h2>
        </div>
        {#if locked}<p class="hint"><Icon name="lock" size={15} />{t('maison.fenetres.rapide.verrouillee_aide')}</p>{/if}
        {#each rows as row (row.key)}
          <div class="row">
            <div class="what">
              <b>{row.title}</b>
              <small class:moving={row.tally.moving}>{row.tally.moving ? t('maison.fenetres.rapide.en_mouvement') : row.said(row.tally)}</small>
            </div>
            <div class="buttons">
              {#each [[row.open, 'open'], [row.air, 'aerate'], [row.close, 'close']].filter(([b]) => b) as [b, what] (what)}
                {@const wet = b.rainy && raining}
                <button class="go" disabled={!!busy || locked || wet} onclick={() => go(row.key, what, b.label)} title={wet ? t('maison.fenetres.rapide.pluie_refuse') : (b.hint ?? b.label)}>
                  <span class="glyph"><Icon name={b.icon} size={26} /></span>
                  <span>{busy === `${row.key}/${what}` ? t('maison.fenetres.rapide.patience') : b.label}</span>
                </button>
              {/each}
              {#if row.tally.moving}
                <button class="stop" disabled={busy === `${row.key}/stop`} onclick={() => go(row.key, 'stop', t('maison.fenetres.rapide.stop_label', { what: row.title }))} aria-label={t('maison.fenetres.rapide.stop_label', { what: row.title })}>
                  <Icon name="stop" size={18} />{t('maison.fenetres.rapide.stop')}
                </button>
              {/if}
            </div>
          </div>
        {/each}
      </section>
    {/if}

    <section class="card rain" class:wet={raining} class:unknown={rain == null} aria-live="polite">
      <div class="card-head">
        <h2><Icon name="water" size={18} />{t('maison.fenetres.pluie.titre')}</h2>
      </div>
      <div class="sky" aria-hidden="true">
        {#if raining}
          <svg viewBox="0 0 120 100">
            <g class="drops">
              {#each [[38, 0], [52, 0.35], [66, 0.15], [80, 0.5], [45, 0.7], [73, 0.85]] as [x, delay] (x + '-' + delay)}
                <line x1={x} y1="62" x2={x - 4} y2="74" style:animation-delay="{delay}s" />
              {/each}
            </g>
            <path class="cloud" d="M34 58a16 16 0 0 1 2-31.8A22 22 0 0 1 77.5 24 17 17 0 1 1 86 58Z" />
          </svg>
        {:else}
          <svg viewBox="0 0 120 100">
            <g class="rays">
              {#each Array.from({ length: 12 }, (_, i) => i * 30) as a (a)}
                <line x1="60" y1="14" x2="60" y2="24" transform="rotate({a} 60 50)" />
              {/each}
            </g>
            <circle class="sun" cx="60" cy="50" r="19" />
          </svg>
        {/if}
      </div>
      <p class="word">
        <b>{rain == null ? '—' : raining ? t('maison.fenetres.pluie.il_pleut') : t('maison.fenetres.pluie.sec')}</b>
        <small>{raining ? t('maison.fenetres.pluie.il_pleut_aide') : t('maison.fenetres.pluie.source')}</small>
      </p>
    </section>

    <section class="card lock" class:on={locked}>
      <span class="ico"><Icon name={locked ? 'lock' : 'lock-open'} size={22} /></span>
      <div class="text">
        <b>{locked ? t('maison.fenetres.rapide.verrouillee') : t('maison.fenetres.rapide.deverrouillee')}</b>
        <small>{locked ? t('maison.fenetres.rapide.verrouillee_detail') : t('maison.fenetres.rapide.deverrouillee_detail')}</small>
      </div>
      {#if locked}
        <button class="toggle" disabled={!!busy || !canUnlock} onclick={() => lock(false)} title={canUnlock ? null : t('maison.fenetres.rapide.sans_cle')}>
          <Icon name="lock-open" size={18} />{busy === 'departure' ? t('maison.fenetres.rapide.patience') : t('maison.fenetres.rapide.deverrouiller')}
        </button>
      {:else}
        <button class="toggle" disabled={!!busy} onclick={() => lock(true)}>
          <Icon name="lock" size={18} />{busy === 'departure' ? t('maison.fenetres.rapide.patience') : t('maison.fenetres.rapide.verrouiller')}
        </button>
      {/if}
    </section>
  </div>
{/if}

<style>
  /* The actions and the lock on the left (half the width at most), the
     rain beside them. */
  .quick {
    display: grid;
    grid-template-columns: repeat(12, minmax(0, 1fr));
    gap: 20px;
    align-items: stretch;
  }

  .actions,
  .lock {
    grid-column: span 5;
  }

  .rain {
    grid-column: span 3;
    grid-row: span 2;
    display: grid;
    grid-template-rows: auto 1fr auto;
    min-height: 290px;
  }

  .actions {
    display: grid;
    gap: 14px;
  }

  .actions .card-head {
    margin-bottom: 0;
  }

  .hint {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0;
    font-size: 13px;
    color: var(--warm-ink);
  }

  /* A group: its name and state on top, its buttons as tiles below. */
  .row {
    display: grid;
    gap: 10px;
  }

  .row + .row {
    padding-top: 14px;
    border-top: 1px solid var(--line, rgba(128, 128, 128, 0.22));
  }

  .what {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .what b {
    font-weight: 650;
  }

  .what small {
    font-size: 12.5px;
    color: var(--ink-3);
  }

  .what small.moving {
    color: var(--cool);
    font-weight: 600;
  }

  /* Opening, (ventilation,) closing side by side, same size, centred. */
  .buttons {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(96px, 1fr));
    gap: 10px;
  }

  button {
    font: inherit;
    cursor: pointer;
  }

  button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .go {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: flex-start;
    gap: 8px;
    padding: 12px 8px;
    border: 0;
    border-radius: 18px;
    text-align: center;
    line-height: 1.2;
    background: var(--surface-2);
    color: var(--ink);
    font-size: 14px;
    font-weight: 650;
    transition: background 0.2s var(--ease);
  }

  .go:not(:disabled):hover {
    background: color-mix(in srgb, var(--cool) 14%, var(--surface-2));
  }

  .glyph {
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface);
    color: var(--cool);
    box-shadow: var(--shadow);
  }

  .stop {
    grid-column: 1 / -1;
    justify-content: center;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border: 0;
    border-radius: 999px;
    background: var(--alert);
    color: #fff;
    font-size: 14px;
    font-weight: 650;
  }

  /* The lock: one line, its button at the end. */
  .lock {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 14px;
  }

  .lock.on {
    background: color-mix(in srgb, var(--warm) 16%, var(--surface));
  }

  .ico {
    flex: none;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface-2);
    color: var(--ink-3);
  }

  .on .ico {
    background: var(--surface);
    color: var(--warm-ink);
  }

  .text {
    flex: 1 1 200px;
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .text b {
    font-weight: 650;
  }

  .text small {
    font-size: 12.5px;
    color: var(--ink-3);
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    padding: 10px 18px;
    border: 0;
    border-radius: 999px;
    background: var(--ink);
    color: var(--surface);
    font-size: 14px;
    font-weight: 650;
  }

  /* The rain: a sun when dry, a cloud with its drops when it rains. */
  .rain .card-head {
    margin-bottom: 0;
  }

  .sky {
    display: grid;
    place-items: center;
    min-height: 0;
  }

  .sky svg {
    width: min(100%, 210px);
    height: auto;
    overflow: visible;
  }

  .sun {
    fill: var(--sun);
  }

  .rays line {
    stroke: var(--sun);
    stroke-width: 5;
    stroke-linecap: round;
  }

  .rays {
    transform-origin: 60px 50px;
    animation: turn 40s linear infinite;
  }

  .cloud {
    fill: color-mix(in srgb, var(--ink-3) 55%, var(--surface));
  }

  .drops line {
    stroke: var(--cool);
    stroke-width: 4;
    stroke-linecap: round;
    animation: fall 1.1s linear infinite;
  }

  .unknown .sky {
    opacity: 0.35;
  }

  .word {
    display: grid;
    gap: 2px;
    margin: 0;
    text-align: center;
  }

  .word b {
    font-size: 20px;
    font-weight: 750;
  }

  .wet .word b {
    color: var(--cool);
  }

  .word small {
    font-size: 12.5px;
    color: var(--ink-3);
  }

  @keyframes turn {
    to {
      transform: rotate(360deg);
    }
  }

  @keyframes fall {
    from {
      transform: translateY(-8px);
      opacity: 0;
    }
    30% {
      opacity: 1;
    }
    to {
      transform: translateY(18px);
      opacity: 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .rays,
    .drops line {
      animation: none;
    }
  }

  /* Narrower screens: actions and rain half and half, the lock below. */
  @media (max-width: 1180px) {
    .actions,
    .rain {
      grid-column: span 6;
      grid-row: auto;
    }

    .lock {
      grid-column: span 12;
    }
  }

  @media (max-width: 760px) {
    .quick > * {
      grid-column: span 12 !important;
    }

    .rain {
      min-height: 0;
      grid-template-columns: auto 1fr;
      grid-template-rows: auto auto;
      align-items: center;
      gap: 0 16px;
    }

    .rain .card-head {
      grid-column: span 2;
    }

    .sky svg {
      width: 96px;
    }

    .word {
      text-align: left;
    }
  }

  @media (max-width: 520px) {
    .toggle {
      flex: 1 1 100%;
      justify-content: center;
    }
  }
</style>
