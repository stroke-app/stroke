<script>
  /**
   * Add a theme: pick one from the community gallery (stroke.click/themes),
   * import a Stroke or VS Code theme file, or paste its JSON. The theme is
   * kept with your imported themes and applied at once.
   */
  import * as Dialog from '$lib/components/ui/dialog/index.js'
  import { Button } from '$lib/components/ui/button/index.js'
  import Icon from './Icon.svelte'
  import SearchField from './SearchField.svelte'
  import { cn, errorText } from '$lib/utils.js'
  import { parseThemeFile } from '$lib/themes/theme-file.js'
  import { addImportedTheme } from '$lib/stores/imported-themes.js'
  import { updateSettings } from '$lib/stores/settings.js'
  import { toast } from '$lib/components/ui/sonner/toast.svelte.js'

  let { open = $bindable(false) } = $props()

  const GALLERY = 'https://stroke.click/themes/index.json'

  /** @typedef {{ id: string, name: string, author?: string, appearance: string, file: string, preview: { bg: string, fg: string, accent: string } }} GalleryItem */
  /** @type {GalleryItem[]} */
  let gallery = $state([])
  let galleryError = $state('')
  let galleryLoading = $state(false)
  let query = $state('')
  let json = $state('')
  let busy = $state(/** @type {string} */ (''))
  /** @type {HTMLInputElement | null} */
  let fileInput = $state(null)

  $effect(() => {
    if (!open || gallery.length || galleryLoading) return
    galleryLoading = true
    galleryError = ''
    fetch(GALLERY, { cache: 'no-store' })
      .then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
      .then((d) => { gallery = Array.isArray(d?.themes) ? d.themes : [] })
      .catch((e) => { galleryError = `Couldn't load the gallery (${e.message ?? e}).` })
      .finally(() => { galleryLoading = false })
  })

  const shown = $derived(
    gallery.filter((t) => {
      const q = query.trim().toLowerCase()
      return !q || t.name.toLowerCase().includes(q) || (t.author ?? '').toLowerCase().includes(q)
    }),
  )

  /** What the pasted JSON reads as, or why it can't be used. */
  const pasted = $derived.by(() => {
    if (!json.trim()) return null
    try { return { theme: parseThemeFile(JSON.parse(json)), error: '' } } catch (e) {
      return { theme: null, error: String(/** @type {any} */ (e)?.message ?? e).replace(/^SyntaxError: /, 'Not valid JSON: ') }
    }
  })

  /** @param {unknown} raw @param {string} [from] */
  function add(raw, from) {
    const theme = parseThemeFile(raw)
    addImportedTheme(theme)
    updateSettings({ theme: 'imported' })
    toast.success(`${theme.name} added`, { description: from ? `From ${from}. It's in Settings > Appearance > Theme.` : "It's in Settings > Appearance > Theme." })
    open = false
    json = ''
  }

  /** @param {GalleryItem} t */
  async function addFromGallery(t) {
    busy = t.id
    try {
      const r = await fetch(new URL(t.file, GALLERY).toString(), { cache: 'no-store' })
      if (!r.ok) throw new Error(`HTTP ${r.status}`)
      add(await r.json(), 'stroke.click/themes')
    } catch (e) {
      toast.error(`Couldn't add ${t.name}`, { description: String(/** @type {any} */ (e)?.message ?? e) })
    } finally {
      busy = ''
    }
  }

  /** @param {Event} e */
  async function onFiles(e) {
    const files = /** @type {HTMLInputElement} */ (e.currentTarget).files
    if (!files?.length) return
    for (const f of files) {
      try { add(JSON.parse(await f.text()), f.name) } catch (err) {
        toast.error(`Couldn't read ${f.name}`, { description: String(/** @type {any} */ (err)?.message ?? err) })
      }
    }
    if (fileInput) fileInput.value = ''
  }
  /** @type {HTMLInputElement | null} */
  let searchEl = $state(null)
</script>

<Dialog.Root bind:open>
  <!-- Open on the theme search: the first focusable thing was the
       stroke.click link, which took the focus ring for no reason. -->
  <Dialog.Content
    class="flex max-h-[min(44rem,88vh)] max-w-xl flex-col gap-0 overflow-hidden p-0"
    onOpenAutoFocus={(e) => { if (searchEl) { e.preventDefault(); searchEl.focus() } }}
  >
    <Dialog.Header class="px-6 pt-6 pb-4">
      <Dialog.Title class="text-ui-xl font-semibold tracking-tight">Add a theme</Dialog.Title>
      <Dialog.Description class="text-ui-xs text-muted-foreground">From the community, a file, or pasted JSON.</Dialog.Description>
    </Dialog.Header>

    <div class="app-scroll min-h-0 flex-1 overflow-y-auto px-6 pb-5">
      <section class="flex flex-col gap-2">
        <div class="flex items-baseline justify-between gap-3">
          <h3 class="text-ui-sm font-medium text-foreground">Community themes</h3>
          <button type="button" class="text-ui-2xs text-muted-foreground hover:text-foreground" onclick={() => void import('@tauri-apps/plugin-opener').then((m) => m.openUrl('https://stroke.click/themes')).catch(() => window.open('https://stroke.click/themes', '_blank'))}>
            stroke.click/themes ↗
          </button>
        </div>
        <SearchField bind:value={query} bind:ref={searchEl} placeholder="Search themes" rank={35} />
        {#if galleryLoading}
          <p class="py-4 text-center text-ui-xs text-muted-foreground"><Icon name="loader-2" class="me-1 inline size-3.5 animate-spin" />Loading themes</p>
        {:else if galleryError}
          <p class="py-3 text-ui-xs text-muted-foreground">{galleryError}</p>
        {:else}
          <ul class="grid grid-cols-2 gap-2">
            {#each shown as t (t.id)}
              <li>
                <button
                  type="button"
                  class="group flex w-full items-center gap-3 rounded-lg border border-border/60 p-2 text-left transition-colors hover:border-border hover:bg-muted/40 disabled:opacity-60"
                  disabled={busy === t.id}
                  onclick={() => void addFromGallery(t)}
                >
                  <span class="flex size-10 shrink-0 items-end justify-end overflow-hidden rounded-md border border-border/50 p-1" style="background:{t.preview.bg}">
                    <span class="size-3 rounded-full" style="background:{t.preview.accent}"></span>
                  </span>
                  <span class="min-w-0 flex-1">
                    <span class="block truncate text-ui-sm font-medium text-foreground">{t.name}</span>
                    <span class="block truncate text-ui-2xs text-muted-foreground">{t.appearance === 'light' ? 'Light' : 'Dark'}{t.author ? ` · ${t.author}` : ''}</span>
                  </span>
                  {#if busy === t.id}<Icon name="loader-2" class="size-3.5 shrink-0 animate-spin text-muted-foreground" />{:else}<Icon name="plus" class="size-3.5 shrink-0 text-muted-foreground opacity-0 group-hover:opacity-100" />{/if}
                </button>
              </li>
            {/each}
          </ul>
          {#if !shown.length}<p class="py-3 text-ui-xs text-muted-foreground">No themes match.</p>{/if}
        {/if}
      </section>

      <div class="my-5 flex items-center gap-3 text-ui-3xs font-medium tracking-wider text-muted-foreground uppercase">
        <span class="h-px flex-1 bg-border/60"></span>Or import<span class="h-px flex-1 bg-border/60"></span>
      </div>

      <section class="flex items-center justify-between gap-4 rounded-lg border border-dashed border-border/70 px-4 py-3">
        <div class="min-w-0">
          <p class="text-ui-sm font-medium text-foreground">Theme file</p>
          <p class="text-ui-2xs text-muted-foreground">A Stroke or VS Code theme, .json</p>
        </div>
        <input bind:this={fileInput} type="file" accept=".json,application/json" multiple class="hidden" onchange={onFiles} />
        <Button variant="outline" size="sm" class="shrink-0 gap-1.5" onclick={() => fileInput?.click()}>
          <Icon name="file-up" class="size-3.5 shrink-0" />Choose files
        </Button>
      </section>

      <section class="mt-4 flex flex-col gap-1.5">
        <label for="theme-json" class="text-ui-sm font-medium text-foreground">Theme JSON</label>
        <textarea
          id="theme-json"
          bind:value={json}
          spellcheck="false"
          placeholder={'{\n  "version": 1,\n  "name": "Aurora",\n  "appearance": "dark",\n  "colors": { "background": "#0f1117", "foreground": "#e6e6e6", "primary": "#7aa2f7" }\n}'}
          class="field-surface app-scroll h-36 w-full resize-none bg-transparent p-3 font-mono text-ui-xs text-foreground placeholder:text-muted-foreground"
        ></textarea>
        {#if pasted?.error}
          <p class="text-ui-2xs text-destructive">{pasted.error}</p>
        {:else if pasted?.theme}
          <p class={cn('flex items-center gap-1.5 text-ui-2xs text-muted-foreground')}>
            <span class="size-2.5 rounded-full border border-border" style="background:{pasted.theme.colors.primary}"></span>
            {pasted.theme.name} · {pasted.theme.appearance === 'light' ? 'light' : 'dark'}
          </p>
        {/if}
      </section>
    </div>

    <Dialog.Footer class="mx-0 mb-0 flex-row gap-2 border-t border-border/40 px-6 py-3 sm:justify-end">
      <Button variant="ghost" size="sm" onclick={() => (open = false)}>Cancel</Button>
      <Button size="sm" class="gap-1.5" disabled={!pasted?.theme} onclick={() => { try { add(JSON.parse(json), 'pasted JSON') } catch (e) { toast.error("Couldn't add the theme", { description: errorText(e) }) } }}>
        <Icon name="plus" class="size-3.5 shrink-0" />Add theme
      </Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
