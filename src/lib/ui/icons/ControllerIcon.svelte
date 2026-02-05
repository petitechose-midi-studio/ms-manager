<script lang="ts">
  export let size = 14;
  export let title: string | null = null;
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 100 100"
  role={title ? "img" : "presentation"}
  aria-label={title ?? undefined}
>
  {#if title}
    <title>{title}</title>
  {/if}

  <rect x="12" y="12" width="76" height="76" rx="10" fill="none" stroke="currentColor" stroke-width="6" />
  <rect x="37.4" y="21.55" width="25.31" height="19.96" rx="2.54" fill="currentColor" opacity="0.18" />

  <circle cx="26.87" cy="24.27" r="2.73" fill="currentColor" opacity="0.55" />
  <circle cx="26.87" cy="31.94" r="2.73" fill="currentColor" opacity="0.55" />
  <circle cx="26.87" cy="39.6" r="2.73" fill="currentColor" opacity="0.55" />

  <circle cx="73.81" cy="24.93" r="3.26" fill="currentColor" opacity="0.55" />
  <circle cx="73.81" cy="38.31" r="4.02" fill="currentColor" opacity="0.55" />

  <circle cx="28.42" cy="60.8" r="3.2" fill="currentColor" />
  <circle cx="42.81" cy="60.8" r="3.2" fill="currentColor" />
  <circle cx="57.19" cy="60.8" r="3.2" fill="currentColor" />
  <circle cx="71.58" cy="60.8" r="3.2" fill="currentColor" />

  <circle cx="28.42" cy="75.18" r="3.2" fill="currentColor" />
  <circle cx="42.81" cy="75.18" r="3.2" fill="currentColor" />
  <circle cx="57.19" cy="75.18" r="3.2" fill="currentColor" />
  <circle cx="71.58" cy="75.18" r="3.2" fill="currentColor" />

  <circle cx="40.13" cy="48" r="2.73" fill="currentColor" opacity="0.35" />
  <circle cx="50" cy="48" r="2.73" fill="currentColor" opacity="0.35" />
  <circle cx="59.98" cy="48" r="2.73" fill="currentColor" opacity="0.35" />
</svg>

<style>
  .icon {
    display: block;
  }
</style>
