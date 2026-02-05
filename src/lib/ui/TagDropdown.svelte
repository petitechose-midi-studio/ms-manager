<script lang="ts" context="module">
  export type TagOption = import("$lib/ui/LabeledSelect.svelte").SelectOption;
</script>

<script lang="ts">
  import LabeledSelect from "$lib/ui/LabeledSelect.svelte";

  export let value: string;
  export let disabled = false;
  export let options: TagOption[];
  export let onChange: (next: string) => void;
</script>

<LabeledSelect label="Version" {value} {disabled} {options} onChange={onChange} />
