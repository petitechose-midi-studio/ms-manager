<script lang="ts" context="module">
  export type TagOption = {
    value: string;
    label: string;
  };
</script>

<script lang="ts">
  import ChoiceDropdown from "$lib/ui/ChoiceDropdown.svelte";

  export let value: string;
  export let disabled = false;
  export let options: TagOption[];
  export let onChange: (next: string) => void;
</script>

<ChoiceDropdown label="Version" {value} {disabled} {options} {onChange} />
