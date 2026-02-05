<script lang="ts">
  import type { Channel } from "$lib/api/types";
  import LabeledSelect, { type SelectOption } from "$lib/ui/LabeledSelect.svelte";

  export let value: Channel;
  export let disabled = false;
  export let onChange: (next: Channel) => void;

  const options: SelectOption[] = [
    { value: "stable", label: "stable" },
    { value: "beta", label: "beta" },
    { value: "nightly", label: "nightly" },
  ];

  function toChannel(v: string): Channel {
    if (v === "stable" || v === "beta" || v === "nightly") return v;
    return "stable";
  }
</script>

<LabeledSelect
  label="Channel"
  value={value}
  {disabled}
  {options}
  onChange={(v) => onChange(toChannel(v))}
/> 
