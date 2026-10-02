<script lang="ts">
  let {
    checked,
    onchange,
    label = "",
    disabled = false,
    title,
  }: {
    checked: boolean;
    onchange: (v: boolean) => void;
    label?: string;
    disabled?: boolean;
    title?: string;
  } = $props();
</script>

<label class="toggle" class:on={checked} class:disabled {title}>
  <input
    type="checkbox"
    role="switch"
    aria-checked={checked}
    aria-label={label ? undefined : title}
    {checked}
    {disabled}
    onchange={(e) => onchange(e.currentTarget.checked)}
  />
  <span class="track" aria-hidden="true"><span class="knob"></span></span>
  {#if label}<span class="label">{label}</span>{/if}
</label>

<style>
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
    white-space: nowrap;
    color: var(--muted);
    font-size: 12px;
  }
  .toggle.on {
    color: var(--text);
  }
  .disabled {
    opacity: 0.45;
    cursor: default;
  }
  input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
    margin: 0;
  }
  .track {
    position: relative;
    width: 30px;
    height: 16px;
    flex: none;
    background: #010a13;
    border: 1px solid var(--line-gold);
    transition: background 0.15s, border-color 0.15s;
  }
  .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 10px;
    height: 10px;
    background: var(--faint);
    transition: transform 0.15s, background 0.15s;
  }
  .on .track {
    background: linear-gradient(180deg, #8a6a2c, #5b4519);
    border-color: var(--gold-hi);
  }
  .on .knob {
    transform: translateX(14px);
    background: var(--text);
    box-shadow: 0 0 6px rgba(240, 230, 210, 0.6);
  }
  input:focus-visible + .track {
    outline: 1px solid var(--gold-hi);
    outline-offset: 2px;
  }
</style>
