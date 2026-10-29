<script>
  // The lights of the heaters' control boxes, through their night mode
  // (Tuya's `night_btn`: on, the box's LEDs go dark during its night):
  // « Éteindre » sets every module's night from 00:00 to 23:59 and turns
  // it on, « Allumer » turns it off. Said how many are dark and which
  // differ, then one order per module that is not yet as asked, one after
  // the other (they share their gateway).
  import { hub, value, act, note, nameOf, reachable } from '../lib/home.svelte.js';
  import { t } from '../../lib/i18n.svelte.js';
  import Icon from './Icon.svelte';

  /** The whole day, in the modules' minutes since midnight. */
  const FROM = 0;
  const TO = 23 * 60 + 59;

  const modules = $derived(
    Object.values(hub.devices)
      .filter((d) => d.points.some((p) => p.key === 'night_btn' && p.access?.write))
      .sort((a, b) => nameOf(a.id).localeCompare(nameOf(b.id))),
  );
  const writes = (d, key) => d.points.some((p) => p.key === key && p.access?.write);
  /** Dark all day: night on, from 00:00 to 23:59 (hours it cannot change
   *  are taken as they are). */
  const allDay = (d) =>
    (!writes(d, 'night_time_start') || value(d.id, 'night_time_start') === FROM) && (!writes(d, 'night_time_end') || value(d.id, 'night_time_end') === TO);
  const dark = $derived(modules.filter((d) => value(d.id, 'night_btn') === true));
  const lit = $derived(modules.filter((d) => value(d.id, 'night_btn') === false));
  // Dark, but only for part of the day (hours the module said: unknown
  // ones are written anyway): « Éteindre » makes it all day.
  const said = (d) => typeof value(d.id, 'night_time_start') === 'number' && typeof value(d.id, 'night_time_end') === 'number';
  const partly = $derived(dark.filter((d) => said(d) && !allDay(d)));
  let busy = $state(null);

  /** The orders `d` needs to be as asked (`off`: its LEDs dark all day). */
  function orders(d, off) {
    if (!off) return value(d.id, 'night_btn') === false ? [] : [['night_btn', false]];
    const out = [];
    if (writes(d, 'night_time_start') && value(d.id, 'night_time_start') !== FROM) out.push(['night_time_start', FROM]);
    if (writes(d, 'night_time_end') && value(d.id, 'night_time_end') !== TO) out.push(['night_time_end', TO]);
    if (value(d.id, 'night_btn') !== true) out.push(['night_btn', true]);
    return out;
  }

  async function all(off) {
    busy = off ? 'off' : 'on';
    const left = modules.filter((d) => orders(d, off).length);
    const todo = left.filter((d) => reachable(d.id));
    const missed = left.length - todo.length;
    const failed = [];
    for (const d of todo) {
      const label = t(off ? 'commun.nuit.label_eteindre' : 'commun.nuit.label_allumer', { name: nameOf(d.id) });
      for (const [key, wanted] of orders(d, off)) {
        const done = await act(`${d.id}/${key}`, wanted, label);
        // Held for an approval: the guard says it, the others go on.
        if (done === 'error') {
          failed.push(nameOf(d.id));
          break;
        }
      }
    }
    busy = null;
    if (failed.length) note(t('commun.nuit.pas_tous', { count: failed.length, names: failed.join(', ') }), 'error');
    else if (todo.length) note(t(off ? 'commun.nuit.note_eteintes' : 'commun.nuit.note_allumees', { count: todo.length }));
    if (missed > 0) note(t('commun.nuit.injoignables', { count: missed }), 'error');
  }

  const allDark = $derived(dark.length === modules.length && dark.every(allDay));
</script>

{#if modules.length}
  <section class="night" class:on={dark.length === modules.length} aria-live="polite">
    <span class="ico"><Icon name="light" size={20} /></span>
    <div class="text">
      <small class="eyebrow">{t('commun.nuit.surtitre')}</small>
      <b>{t('commun.nuit.titre')}</b>
      <small>
        {#if dark.length === modules.length}{t('commun.nuit.partout', { count: modules.length })}
        {:else if lit.length === modules.length}{t('commun.nuit.nulle_part', { count: modules.length })}
        {:else}{t('commun.nuit.sur', { n: dark.length, count: modules.length })}{/if}
      </small>
      {#if dark.length && lit.length}
        <small class="which">{t(dark.length >= lit.length ? 'commun.nuit.sauf' : 'commun.nuit.seulement', { names: (dark.length >= lit.length ? lit : dark).map((d) => nameOf(d.id)).join(', ') })}</small>
      {/if}
      {#if partly.length}
        <small class="which">{t('commun.nuit.en_partie', { names: partly.map((d) => nameOf(d.id)).join(', ') })}</small>
      {/if}
    </div>
    <div class="buttons">
      <button class="go" disabled={busy != null || lit.length === modules.length} onclick={() => all(false)}>
        {busy === 'on' ? t('commun.nuit.patience') : t('commun.nuit.allumer')}
      </button>
      <button class="go soft" disabled={busy != null || allDark} onclick={() => all(true)}>
        {busy === 'off' ? t('commun.nuit.patience') : t('commun.nuit.eteindre')}
      </button>
    </div>
  </section>
{/if}

<style>
  .night {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border-radius: var(--r-md);
    background: var(--surface-2);
  }

  .night.on {
    background: color-mix(in srgb, var(--cool) 14%, var(--surface));
  }

  .ico {
    flex: none;
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--surface);
    color: var(--ink-3);
  }

  .on .ico {
    color: var(--cool);
  }

  .text {
    flex: 1 1 180px;
    min-width: 0;
    display: grid;
    gap: 2px;
  }

  b {
    font-weight: 650;
    color: var(--ink);
  }

  small {
    font-size: 12.5px;
    color: var(--ink-3);
  }

  .eyebrow {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  .buttons {
    display: flex;
    gap: 8px;
    margin-left: auto;
  }

  .go {
    flex: none;
    border: 0;
    border-radius: 999px;
    padding: 9px 16px;
    font: inherit;
    font-size: 14px;
    font-weight: 650;
    background: var(--ink);
    color: var(--surface);
    cursor: pointer;
  }

  .go.soft {
    background: var(--surface);
    color: var(--ink);
  }

  .go:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
