<script>
  import { home, savePref } from '../lib/home.svelte.js';
  import { t } from '../../lib/i18n.svelte.js';
  import Icon from './Icon.svelte';

  /** Day or night for this screen: automatic (night after sunset, by the
   *  house's own daylight), or always one of them. Kept by this device
   *  only, as before when the menu's button chose it. */
  const CHOICES = [
    ['auto', 'commun.theme.auto', 'sun-angle'],
    ['light', 'commun.theme.jour', 'sun'],
    ['dark', 'commun.theme.nuit', 'moon'],
  ];

  function choose(theme) {
    home.theme = theme;
    savePref('maison-theme', theme);
  }
</script>

<section class="card theme">
  <span class="icon"><Icon name={home.theme === 'dark' ? 'moon' : 'sun'} size={20} /></span>
  <div class="what">
    <b>{t('systeme.theme.titre')}</b>
    <span class="muted">{t('systeme.theme.aide')}</span>
  </div>
  <div class="seg" role="radiogroup" aria-label={t('systeme.theme.titre')}>
    {#each CHOICES as [value, label, icon] (value)}
      <button role="radio" aria-checked={home.theme === value} class:on={home.theme === value} onclick={() => choose(value)}>
        <Icon name={icon} size={16} />{t(label)}
      </button>
    {/each}
  </div>
</section>

<style>
  .theme {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 16px;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: 50%;
    background: var(--cool-soft);
    color: var(--cool);
  }

  .what {
    display: grid;
    flex: 1;
    min-width: 220px;
    gap: 2px;
    font-size: 14px;
  }

  .seg {
    display: inline-flex;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);
  }

  .seg button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 14px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: var(--ink-3);
    font: inherit;
    font-size: 14px;
    font-weight: 650;
    cursor: pointer;
  }

  .seg button.on {
    background: var(--surface);
    color: var(--ink);
    box-shadow: 0 1px 3px rgb(0 0 0 / 15%);
  }
</style>
