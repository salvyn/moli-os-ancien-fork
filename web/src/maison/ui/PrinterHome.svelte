<script>
  import { home, device, value, nameOf, num } from '../lib/home.svelte.js';
  import { onMinutes, onLevel } from '../lib/paper.js';
  import { duration } from '../lib/printers.js';
  import { t } from '../../lib/i18n.svelte.js';
  import Icon from './Icon.svelte';

  /** The paper printer on the home page while it is on: its state, how long
   *  it has been on (a worry after 12 hours), its toner or ink. The whole
   *  card, with printing, is on the Printing page. */
  let { id } = $props();

  const d = $derived(device(id));
  const off = $derived(d?.online !== true);
  const status = $derived(off ? t('salon.eteinte') : (value(id, 'status') ?? t('salon.papier.prete')));
  const STUCK = /papier|bourrage|capot|vide ou absente|arrêtée|paper|jam|cover|empty or missing|stopped/i;
  const level = $derived(onLevel(d, home.now));
  const minutes = $derived(onMinutes(d, home.now));
  const tone = $derived(off ? '' : STUCK.test(status) ? 'alert' : level === 'too_long' ? 'alert' : level === 'long' || value(id, 'ink_low') ? 'warm' : 'good');
  const inks = $derived(
    (d?.points ?? [])
      .filter((p) => /^ink_\d+$/.test(p.key))
      .map((p) => ({ key: p.key, label: p.label, level: value(id, p.key) }))
      .filter((i) => typeof i.level === 'number'),
  );
</script>

<section class="card printer-home">
  <div class="card-head">
    <h2><Icon name="printer" size={18} /><span class="name">{nameOf(id)}</span></h2>
    <a class="more" href="#/impression" aria-label={t('maison.accueil.imprimante_page')}><Icon name="arrow-top-right" size={18} /></a>
  </div>
  <div class="state"><span class="chip {tone}">{status}</span></div>
  <dl class="figures">
    {#each inks as ink (ink.key)}
      <div class="fig" class:low={ink.level <= 15}>
        <dt>{ink.label}</dt>
        <dd class="num">{num(ink.level, 0)} %</dd>
      </div>
    {/each}
    <div class="fig" class:alert={level === 'too_long'} class:warm={level === 'long'}>
      <dt>{t('maison.accueil.imprimante_depuis')}</dt>
      <dd class="num">{#if minutes != null}{duration(minutes)}{:else}<span class="muted">—</span>{/if}</dd>
    </div>
  </dl>
</section>

<style>
  .printer-home {
    display: grid;
    gap: 10px;
  }

  /* Under the energy card, in the home page's side column. */
  .printer-home:not(:first-child) {
    margin-top: 20px;
  }

  /* The name on one clean line, the link to the printer page at its end. */
  .printer-home .card-head {
    margin-bottom: 0;
    justify-content: flex-start;
    gap: 10px;
  }

  .card-head h2 {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .state {
    display: flex;
  }

  .state .chip {
    max-width: 100%;
  }

  /* The figures along the bottom: label above, value below, one column each. */
  .figures {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: minmax(0, 1fr);
    gap: 8px 14px;
    margin: 0;
    padding-top: 10px;
    border-top: 1px solid var(--line, rgba(128, 128, 128, 0.22));
  }

  .fig {
    display: grid;
    gap: 2px;
    min-width: 0;
    text-align: center;
    color: var(--ink-2);
  }

  .fig dt {
    font-size: 12px;
    color: var(--ink-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .fig dd {
    margin: 0;
    font-size: 15px;
    font-weight: 650;
    white-space: nowrap;
  }

  .fig.warm dd {
    color: var(--warm-ink);
  }

  .fig.alert dd,
  .fig.low dd {
    color: var(--alert);
  }

  .muted {
    color: var(--ink-3);
  }
</style>
