<script lang="ts">
  // 0-5 stars, drawn as an empty row with a filled row clipped on top.
  let { value }: { value: number } = $props()
</script>

<span class="rating" aria-label="{value} out of 5">
  <span class="empty">★★★★★</span>
  <span class="full" style:width="{(value / 5) * 100}%">★★★★★</span>
</span>

<style>
  .rating {
    position: relative;
    display: inline-block;
    font-size: 16px;
    letter-spacing: 2px;
    line-height: 1;
  }
  .empty {
    color: rgb(255 255 255 / 0.18);
  }
  .full {
    position: absolute;
    left: 0;
    top: 0;
    overflow: hidden;
    white-space: nowrap;
    color: var(--accent);
  }
</style>
