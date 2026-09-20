# Crafty web UI conventions

The Svelte UI lives in `app/frontend/web`. It is a SvelteKit single-page app (Svelte 5 runes, TypeScript, Tailwind v4, shadcn-svelte on Bits UI). `pnpm build` writes it to `app/frontend/spa`, and Tornado serves that folder at `/ui/` (see `app/classes/web/spa_handler.py`). The template UI under `/panel/*` keeps working while pages are rebuilt.

Reference docs in this folder:

- `legacy-ui-inventory.md`: every feature of the template UI, its endpoints and WebSocket events. This is the parity checklist.

## Running it

- Backend: from the repo root, `.venv/Scripts/python.exe main.py -d -i` (use `uv` to create the venv). It listens on `https://localhost:8443`.
- Frontend: `pnpm dev --port <port>` in `app/frontend/web`, then open `http://localhost:<port>/ui/`. Vite proxies `/api`, `/ws` and `/static` to Crafty (`CRAFTY_URL` overrides the target).
- Log in at `/ui/login`. The first-run admin password is in `app/config/default-creds.txt`.
- Checks: `pnpm check` must report 0 errors. Run `npx @sveltejs/mcp svelte-autofixer <file>` on every `.svelte` / `.svelte.ts` file you write.

## Design language

The look follows Vercel's dashboard (Geist): neutral grays, hairline borders, black or white primary buttons, little color.

- Use theme tokens only: `bg-background`, `bg-card`, `bg-muted`, `text-muted-foreground`, `border`, `text-destructive`, `bg-success`, `text-warning`, `text-info`. No raw hex colors (the console terminal is the one exception).
- Fonts: Geist for UI text, Geist Mono (`font-mono`) for paths, IDs, ports, commands and code. Use `tabular-nums` for changing numbers.
- Page container: `<div class="mx-auto w-full max-w-6xl px-4 py-8 md:px-8">`. Server sub-pages use `py-6`, because the server header sits above them.
- Page titles: `PageHeader` (`$lib/components/page-header.svelte`). Server sub-pages don't need a title; the tabs name the page.
- Settings forms: `SettingsCard` (`$lib/components/settings-card.svelte`). This is Vercel's pattern: title, description, fields, and a muted footer with a hint on the left and the Save button on the right. Use one card per group of related settings, and `destructive` for danger zones.
- Lists: a `Table` wrapped in `<div class="overflow-hidden rounded-lg border">`, with the header row `class="bg-muted/50 hover:bg-muted/50"`. Put row actions in a `…` dropdown (`EllipsisIcon`, ghost `icon-sm` button) at the end of the row.
- Empty states: `Empty` components inside `rounded-lg border border-dashed`.
- Loading: `Skeleton` shapes that match the final layout, not spinners. Use `Spinner` only inside busy buttons.
- Stats: bordered tiles separated by 1px lines (see the Overview page) and `Meter` for usage bars.
- Status: `StatusDot` / `StatusBadge`.
- Icons: import one icon per file, for example `import PlayIcon from '@lucide/svelte/icons/play'`.
- Wording: short and plain. Button labels are verbs ("Create backup", "Save"). Sentence case everywhere.
- Accessibility: every icon-only button has `aria-label`, and form fields have labels (`Field.Label` / `Label`). Everything must work with the keyboard.
- Mobile: layouts must work at 375px width. Hide less important table columns with `hidden md:table-cell`.

## Code

- Svelte 5 runes only: `$state`, `$derived`, `$effect` (rarely), `$props`, snippets, `onclick=`. No stores and no `export let`.
- Use `$state.raw` for API responses that you replace rather than mutate.
- Use keyed `{#each}`.
- Default to no comments. Only add a one-line comment when the code cannot show why.
- Formatting: tabs, single quotes, no trailing commas (Prettier config in the repo).
- Links: `resolve('/path')` from `$app/paths`. Server routes use template pathnames: ``resolve(`/servers/${id}/files`)``.

### Shared modules (don't change these without coordinating)

- `$lib/api/client`: `api.get/post/patch/put/delete`. It unwraps the `{status, data}` envelope and throws `ApiError` (`status`, `code`, `detail`, `payload` = the whole error body). Endpoints without an envelope return the raw payload. A `Blob` or `FormData` body is sent as-is.
- `$lib/api/servers`: `errorMessage(err)`, `powerAction()`, `sendCommand()`.
- `$lib/api/types`: shared types. Put new area types in your own module, such as `$lib/api/backups.ts`.
- `$lib/session.svelte`: `session.info`, `session.user`, `session.superuser`, `session.can('SERVER_CREATION' | 'USER_CONFIG' | 'ROLES_CONFIG')`.
- `$lib/servers.svelte`: `servers.list`, `servers.get(id)`, `servers.metrics(id)`, `servers.statusOf(id)`, `servers.refresh()`.
- `$lib/realtime/socket.svelte`: use `socket.on(event, cb)` inside `$effect` and return the unsubscribe function. Server pages are already subscribed to that server's events (`/panel/server_detail` with `id`) by the server layout. Other pages that need page-scoped events call `useSocketPage(() => ({ page: '/panel/…', params: {…} }))`.
- `$lib/components/confirm/confirm.svelte`: `await confirm({ title, description, confirmLabel, destructive, checkbox })`. Use it for every destructive action; `confirmWith()` returns `{ confirmed, checked }` when you pass a `checkbox`.
- Toasts: `toast`, `toast.success`, `toast.error(title, { description: errorMessage(err) })` from `svelte-sonner`.
- `$lib/format`: `memory`, `percent`, `duration`, `relativeTime`, `parseUtc`, `stripMotd`, `serverTypeLabel`.

### Server pages

`src/routes/(app)/servers/[id]/+layout.ts` loads `data.server` (`ServerDetail`: settings, stats, flags, and the caller's `permissions`). Each tab has a `+page.ts` that appends to `crumbs`. Gate actions on `data.server.permissions.includes('<PERMISSION>')`. Tab visibility is defined in `$lib/server-tabs.ts`.

## Backend endpoints for the UI

- New endpoints go in `app/classes/web/routes/api/ui/<area>_routes.py`, in its `routes(handler_args)` list, with URLs under `/api/v2/ui/<area>/…`. They are already registered in `api_handlers.py`.
- Follow the pattern in `app/classes/web/routes/api/ui/index.py`:
  - `auth_data = self.authenticate_user()`.
  - Access check: `server_id in [str(x["server_id"]) for x in auth_data[0]]`.
  - Permission mask: `get_lowest_api_perm_mask(get_user_permissions_mask(user_id, server_id), auth_data[5])`.
  - Response: `self.finish_json(status, {"status": "ok", "data": ...})`.
- Never change the response shape of an existing endpoint that the template UI uses. Search `app/frontend/templates` and `app/frontend/static/assets/js` first. Add a `/api/v2/ui/...` endpoint instead.
- New endpoints need a Crafty restart before they respond.

## Text and translation

Never put user-visible English in a component. Call `t()` from `$lib/i18n/index.svelte`:

```svelte
<script lang="ts">
	import { t, plural } from '$lib/i18n/index.svelte';
</script>

<h1>{t('dashboard.title')}</h1>
<p>{t('dashboard.runningCount', { running, total })}</p>
<p>{plural('files.selected', count)}</p>
```

- English lives in `src/lib/i18n/messages/<area>.ts`, registered in `messages/index.ts`. Add keys to
  the catalog for your area, and reuse `common.*` (actions, states, server statuses, permissions)
  rather than repeating them.
- Placeholders are `{name}`. Counts use `<key>_one` / `<key>_other` with `plural()`.
- Translate labels, headings, placeholders, `aria-label`, `title`, tooltips, empty states, dialog and
  confirm copy, toasts and validation text. Do not translate data from the API, file paths, console
  output or API error messages (already localised server-side).
- Breadcrumbs from a `+page.ts` use `{ labelKey: 'nav.tabs.files' }`; use `label` only for names that
  come from data, such as a server's name.
- Format dates and numbers with `i18n.tag` (a BCP 47 tag) so they follow the chosen language.
- Other languages are flat JSON overrides in `app/translations/ui/<code>.json` with English fallback;
  `pnpm i18n:keys` regenerates `_template.json` for translators.
