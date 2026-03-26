<script lang="ts">
  import type { Channel } from "$lib/api/types";
  import ChoiceDropdown from "$lib/ui/ChoiceDropdown.svelte";

  export let value: Channel;
  export let disabled = false;
  export let onChange: (next: Channel) => void;

  function toChannel(value: string): Channel {
    if (value === "stable" || value === "beta" || value === "nightly") return value;
    return "stable";
  }
</script>

<ChoiceDropdown
  label="Channel"
  {value}
  {disabled}
  options={[
    { value: "stable", label: "Stable" },
    { value: "beta", label: "Beta" },
    { value: "nightly", label: "Nightly" },
  ]}
  onChange={(next) => onChange(toChannel(next))}
/>
