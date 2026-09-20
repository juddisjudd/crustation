# Crafty 4 web UI: feature inventory for the Svelte SPA rewrite

Every template, handler and static JS file was read. Paths are relative to the repo root `S:\_projects_\_minecraft_\crafty-4\`, with these shorthands:

- **T** = `app\frontend\templates\`
- **PH** = `app\classes\web\panel_handler.py`
- **SH** = `app\classes\web\server_handler.py`
- **JS** = `app\frontend\static\assets\js\`
- **API** = `app\classes\web\routes\api\`

Some API endpoints you would expect the SPA to use are broken or missing. They are marked **⚠API** and collected in section 11. Data that only reaches the page through server-side rendering (page_data) is marked **SSR-only**.

---

## 0. Routing, auth, conventions

**Routes** (`app\classes\web\tornado_handler.py:138-151`)

- `/` → DefaultHandler. With no subpath it redirects to `/panel/dashboard` (`default_handler.py:10-25`).
- `/panel/(.*)` → PanelHandler (GET only).
- `/server/(.*)` → ServerHandler.
- `/ws` → WebSocketHandler.
- `/status` → StatusHandler.
- `/api/v2/...` routes are listed in `API\api_handlers.py`. OpenMetrics routes also exist.
- Everything else → PublicHandler.
- `login_url=/login`. `xsrf_cookies=True`, but every API handler disables the XSRF check (`app\classes\web\base_api_handler.py:18-19`). Page JS sends `X-XSRFToken` or a bogus `token:` header; neither matters.

**Auth**

- Token comes from the `token` cookie, the `?token=` query argument, or `Authorization: Bearer` (`app\classes\web\base_handler.py:188-207`).
- `@tornado.web.authenticated` redirects to `/login?next=…`.
- API returns 403 with `otp/mfaWarn` plus a link to `/panel/edit_user_otp?id=` when the `superMFA` setting or a role's `mfa_required` demands MFA and the token has no `mfa` claim. Exempt: TOTP setup routes, API keys, and `anti-lockout-user` (`base_handler.py:223-314`).

**Superuser logic**

- `superuser = user.superuser and (api_key.full_access if logged in via API key)` (PH:281-283).
- Templates use both `data['superuser']` and `data['super_user']`.
  - In PH both keys are the effective (API-key-adjusted) value.
  - In SH, `super_user` is the raw DB flag and `superuser` is the effective value (SH:114, 147).

**PanelHandler render flow** (PH:266-1894)

- If Crafty is still starting, every page renders `panel/loading.html` (PH:272-273, 1885-1886).
- If the user is `anti-lockout-user`, every page becomes `panel_config` (PH:347-348).
- Unknown page → `panel/denied.html`, which is the default template (PH:271).
- Context passed to templates: `translate`, `json`, `time`, `utc_offset` (PH:1887-1894).

**Common page_data built on every /panel page** (PH:350-407). All of it is **SSR-only** unless noted:

- `mfa`: token claim. No API.
- `password_auth_disabled`.
- `update_available`: Crafty update. Only visible through the announcements feed.
- `support_perm` = `general_user_log_access` setting or superuser.
- `docker`.
- `background` (cached_login), `brand` (cached_brand), `login_opacity`.
- `serverTZ`, `monitored` (monitored_mounts setting).
- `version_data`: also available from `GET /api/v2/`.
- `failed_servers`.
- `user_data`: available from `GET /api/v2/users/@me`, which returns the full user minus password and valid_tokens_from, plus role objects.
- `user_role` (role names).
- `user_crafty_permissions`: available from `/users/@me/permissions`.
- `crafty_permissions` enum map.
- `server_stats` {total, running, stopped}.
- `menu_servers`: `/api/v2/servers` returns the list; ordering must be applied client-side from `server_order`.
- `hosts_data`: `/api/v2/crafty/stats`, but `disk_json` comes back as a single-quoted string.
- `show_contribute`, `lang`, `lang_page`.
- `api_key` (name, perms, full_access when logged in with an API key). No API.
- `themes`. No API.

**Permission enums**

- Server permissions, in bit/mask order: `COMMANDS, TERMINAL, LOGS, SCHEDULE, BACKUP, FILES, CONFIG, PLAYERS` (`app\classes\models\server_permissions.py:38-46`).
- Crafty permissions: `SERVER_CREATION, USER_CONFIG, ROLES_CONFIG` (`app\classes\models\crafty_permissions.py:36-39`).
- Masks are "0"/"1" strings in that order.

---

## 1. Shell: base.html, main_menu.html, notify.html, footer.html

**T\base.html**

- Top bar (85-108):
  - Brand logos: `data['brand']['full']` and `data['brand']['square']` (87-90).
  - Sidebar minimize toggle.
  - `#server-name-nav` badge, filled by server pages.
  - notify.html is included here.
  - Mobile offcanvas toggle.
- `<html class="{{theme}}" data-username>` (2-3).
- Meta tags `mfa_warn`, `userId`, and the Hytale auth translation strings are emitted when the token has no MFA and password auth is not disabled (8-13).
- Loads CSS for every theme (58-60), plus the libraries used across pages (jQuery, Alpine, bootstrap-toggle, Chart.js with zoom, DataTables, bootbox).
- The DataTables i18n is injected via a JS-comment trick (170-172): `translate('datatables','i18n')` returns a JSON object.
- Global helpers:
  - `warn()` banner (461-503).
  - `notify()` toast using the square logo (512-570). It is bound to the WS `notification` event (571).
  - `eulaAgree()` → `POST /api/v2/servers/{id}/action/eula/` (438-458).
- Anti-lockout banner (580-591).
- Service worker registration at `/static/assets/js/shared/service-worker.js` (592-598).

**WebSocket client** (base.html:199-310)

- Connects only when `request.protocol=='https'`. Otherwise shows a warning.
- URL: `wss://host/ws?page=<location.pathname>&page_query_params=<encoded location.search>`.
- Exponential backoff reconnect. "WebSockets required" banner after 7s disconnected. Closes after 10s with the tab hidden and reopens when visible.
- API: `webSocket.on(event, cb)` and `emit`. Server `on_message` only logs; clients cannot send anything meaningful (`app\classes\web\websocket_handler.py:106-109`).
- Global listeners:
  - `send_error` → bootbox, then reload (313).
  - `download_progress` (329).
  - `hytale_auth` → bootbox with auth link, then redirect to the server's terminal (342-383).
  - `support_status_update` → `#logs_progress_bar` (385).
  - `send_logs_bootbox` (394). Never emitted.
  - `send_eula_bootbox` → confirm, then `eulaAgree` (404-435).
  - `notification` (571).
  - `zip_status` → file table refresh or progress (600-612).

**T\main_menu.html** (sidebar). Collapse state is stored in localStorage `crafty-sidebar-expanded` (48-52, and `JS\shared\misc.js:185-210`).

| Item                           | Target                                 | Condition                                        |
| ------------------------------ | -------------------------------------- | ------------------------------------------------ |
| Dashboard                      | /panel/dashboard (58-63)               | none                                             |
| Servers (collapsible)          | (65-89)                                | none                                             |
| → Create New Server            | /server/step1 (74-79)                  | `Server_Creation in user_crafty_permissions`     |
| → one entry per `menu_servers` | /panel/server_detail?id= (80-85)       | JS trims the list to the first 11 `li` (136-140) |
| Documentation                  | https://docs.craftycontrol.com (91-96) | none                                             |
| In-App Docs                    | /panel/wiki (98-103)                   | none                                             |
| Discord                        | external link (105-110)                | none                                             |
| Credits                        | /panel/credits (112-117)               | none                                             |
| Contribute                     | /panel/contribute (119-126)            | `show_contribute`                                |
| Panel Settings                 | /panel/panel_config (127-132)          | none                                             |

**T\notify.html**

- Announcements dropdown (1-14). The icon turns red when `update_available`. A count badge is kept in localStorage `notif-count`.
  - `GET /api/v2/crafty/announcements/`, re-polled every 30 min (186-205).
  - Items show title, date, desc, link, and a clear (×) button → `POST /api/v2/crafty/announcements/ {id}` (206-224).
  - The feed also contains migration warnings and the Crafty-update notice (`app\classes\helpers\helpers.py:804-833`).
- User dropdown (16-72):
  - pfp, with fallback to `faces-clipart/pic-3.png` on error.
  - Username, role names, "Logged in as API key" line, email.
  - MFA warning link to `/panel/edit_user_otp?id=self`.
  - Account settings → `/panel/edit_user?id=self`.
  - **Support logs**, only if `support_perm`. When `user_data.preparing` a progress bar is shown instead. Otherwise it opens `/api/v2/crafty/support_logs` in a new tab (226-228).
  - **Activity log** → /panel/activity_logs, only if `data['superuser']` (63-68).
  - Logout → `/logout`.
- MFA link colour adapts to light/dark themes (251-276).

**T\footer.html**: copyright line, `version_data`, and an "Update Available!" link to GitLab releases when `update_available`.

**T\blank_base.html**: chrome-less layout used by the file editor and the orphaned setup1 page. **T\public_base.html**: auth-background layout used by status.html; it applies the login opacity. **T\blank_page_template.html**: an unused scaffold.

---

## 2. Public pages (PublicHandler / StatusHandler)

**Public page_data** (`app\classes\web\public_handler.py:17-28`): `version`, `error`, `lang` = **global** `language` setting, `lang_page`, `query` (from `?next=`, unused), `background`, `brand`, `login_opacity`, `themes`, `embed` (OG metadata). All **SSR-only**. There is **no public branding API**.

**/login → T\public\login.html**

- Background: `/static/assets/images/auth/{background}` with fallback `login_1.jpg` (37-44). Form background opacity comes from `login_opacity` (157-159). Logo is `brand.full`.
- Form fields: username, password, 2FA code, and a type select `totp|backup_code`, which changes the placeholder (94-97, 177-185).
- Submit → `POST /api/v2/auth/login {username,password,[totp|backup_code]}` (186-236).
  - On ok: redirect to `data.page`. A bootbox is shown if `data.warning` is set (burned backup code).
  - Cooldown: `cooldown_time` mm:ss with a live countdown (229-253).
- Passkey login: the button is shown only if WebAuthn exists and `POST /api/v2/auth/passkey/login/options/` returns ok (257-275). Flow: `navigator.credentials.get` → `POST /api/v2/auth/passkey/login/verify/` → `/panel/dashboard` (277-350). Helpers are in `JS\shared\passkey.js`.
- "Forgot password" → `GET /api/v2/crafty/resetPass/`, result shown in a bootbox (anti-lockout; see section 14) (116-118, 168-176).
- Link to `/status`. OG meta via `og_meta.html`.

**/status → T\public\status.html** (StatusHandler GET, `app\classes\web\status_handler.py:9-39`)

- SSR list of all servers where `show_status` is true.
- Desktop table: server name, players online/max, MOTD (motd.js `§` parser, base64 icon), version, online/offline.
- Mobile: accordion.
- Refreshes every 30s via `GET /api/v2/servers/status` (unauthenticated) (263-280).
- **⚠API**: `/servers/status` has no `server_name` field (`API\servers\server\status.py:13-24`), so names are SSR-only.
- StatusHandler also has a POST that renders the same template with incomplete data (41-58). This looks like a legacy path.

**Error pages**

- `/404` and fallback → `T\public\404.html`.
- `/error` and `/panel/error?error=` → `T\public\error.html`. Shows `data['error']` and a "Return" button to the dashboard. Nearly every PH/SH permission failure redirects here with `?error=<text>`.
- `/offline` → `T\public\offline.html` (PWA offline page).
- `/panel/unauthorized` or an unknown panel page → `T\panel\denied.html`.

**/logout** (`public_handler.py:51-60`): clears the `token` cookie. If the user is anti-lockout-user it also deletes that account. There is **no logout API**. `POST /api/v2/auth/invalidate_tokens` invalidates all of the user's tokens.

**T\public\og_meta.html**: renders `og:type`, `og:title`, `og:description`, `og:image`, `og:url`, `theme-color`, and `twitter:card` when embeds are enabled (`app\classes\helpers\embed_helpers.py:12-45`). **Crawlers do not run JS, so the SPA's index.html must still be server-injected with these for /login and /status.**

---

## 3. Dashboard: /panel/dashboard → T\panel\dashboard.html (PH:460-565)

**SSR data**

- `servers`: all server stats for a superuser, otherwise authorized servers. Each entry carries `stats.importing`, `crashed`, `waiting_start`, `alert` (last_backup_failed), `update` (update available and `update_watcher`), and `user_command_permission` (`app\classes\controllers\servers_controller.py:371-426`).
- `dashboard_columns` from the user record.
- `server_stats` totals, `num_players` (0; filled by WS), `failed_servers`, `hosts_data`, `monitored`.
- `first_log`: true for the "admin" user on the first login after a fresh install (PH:461-463).
- **No aggregate dashboard API.** `/api/v2/servers` plus per-server `/stats` and `/servers/{id}` (status {update_available, updating, backing_up, last_backup}) cover only part of it. importing, crashed, waiting_start, failed_servers and totals are SSR-only.

**UI**

- One-time feedback survey bootbox with a Microsoft Forms iframe when `first_log` (26-46).
- Host card:
  - CPU usage, with a tooltip for cores, current and max frequency.
  - Memory %, with a used/total tooltip.
  - Server totals and online/offline counts.
  - Players total and max. Only servers with `count_players` are counted; the others are greyed.
  - Storage bars for disks in `monitored`, colour-coded at ≤58 / 59-75 / >75 (52-144).
- Header actions:
  - **Stop All Servers**: superuser only, and only when anything is running (165-169). Confirm → `POST /api/v2/servers/stop_all` (1039-1079).
  - New Server link (always shown).
- Empty state: welcome text (176-184).
- Server table (desktop, 196-385):
  - Columns: server (link; warning icon if `alert`; cloud-download icon linking to the update center if `update`), serverIcon (base64 or pack.png), actions, cpuUsage, memUsage, size, players, port (`game_port` or `server_port`), version, status (online/crashed/offline).
  - Failed or unloaded servers appear as rows linking to `subpage=config`, with status "Unloaded" (368-383).
- Action buttons, gated by `server['user_command_permission']` (241-290):
  - The button set depends on state: installing, updating, starting (waiting_start, with a delay tooltip), or importing.
  - Running: stop, restart, kill.
  - Stopped: start, clone, kill.
  - start/stop/restart → `POST /api/v2/servers/{id}/action/{start_server|stop_server|restart_server}`, followed by a "be patient" bootbox. After 60s an ad-blocker warning appears (663-687, 883-916).
  - kill: confirm → `kill_server` (917-941).
  - clone: confirm → `clone_server`, then reload (1010-1037).
- Mobile accordion view (388-614).
- **DataTables** search with no paging (1187-1200).
- **Column chooser** (gear icon): `server` and `status` are locked. Visibility is applied via a `<style>` element and persisted with `PATCH /api/v2/users/@me {dashboard_columns:"a,b,..."}` (1204-1312).
- **Drag-reorder rows** (jQuery UI sortable). Disabled while searching or on mobile. Persisted with `PATCH /api/v2/users/@me {server_order:"id,id"}` (1314-1383).
- Hint popovers (`too_small`) when `user_data.hints`.

**WS**: `update_host_stats` (949-981), `send_start_reload` → reload (985), `update_button_status` (991-1003), `update_server_status` (1007). The handler processes only `data[0]` (871-878).

---

## 4. Server detail: /panel/server_detail?id=&subpage= (PH:567-945)

**Access**

- `check_server_id` (PH:144-187): the server must exist or be in `failed_servers`. A superuser passes. An API key needs `server_id_authorized_api_key`. A user needs `server_id_authorized`.
- `SUBPAGE_PERMS` (PH:33-44):

| subpage        | required permission |
| -------------- | ------------------- |
| term           | TERMINAL            |
| logs           | LOGS                |
| schedules      | SCHEDULE            |
| backup         | BACKUP              |
| files          | FILES               |
| config         | CONFIG              |
| admin_controls | PLAYERS             |
| metrics        | **LOGS**            |
| webhooks       | CONFIG              |
| update_center  | CONFIG              |

- The last-visited subpage is remembered **per server, globally across users** (`servers.server_subpage`, PH:574-587).
- With no subpage, the first subpage in the table order that the user has permission for is used. With none, it redirects to an error (PH:687-695).
- Unknown subpage → 404 (PH:696-703).
- No permission and not superuser → error (PH:707-713).

**SSR data for every subpage**

- `server_data` (DB row) and `server_stats` (latest stats; a stub for failed servers, PH:612-648).
- `backup_failed`, `update`, `update_next_run` (scheduler job, PH:594-598).
- `importing`, `waiting_start`, `server_id`.
- `permissions` enum map and `user_permissions` (PH:663-677). **⚠API: no endpoint returns the current user's effective permissions on a server.**
- `server_type`, `crashed`, `active_link`, `serverTZ`.

**Sub-tabs** (`T\panel\parts\server_controls_list.html`; mobile dropdown in `m_server_controls_list.html` with a hard-coded English "Server Controls" label):

| Tab             | Lines | Template condition                                                                  |
| --------------- | ----- | ----------------------------------------------------------------------------------- |
| Terminal        | 2-7   | Terminal perm                                                                       |
| Logs            | 9-14  | Logs perm **and** type != minecraft-bedrock                                         |
| Schedule        | 15-20 | Schedule perm                                                                       |
| Backup          | 21-33 | Backup perm; red with warning icon if `backup_failed`                               |
| Files           | 34-39 | Files perm                                                                          |
| Config          | 40-45 | Config perm                                                                         |
| Player Controls | 46-51 | Players perm **and** type != minecraft-bedrock                                      |
| Metrics         | 52-55 | **no template check** (handler requires LOGS)                                       |
| Webhooks        | 56-61 | Config perm                                                                         |
| Update Center   | 62-69 | Config perm **and** type != steam_cmd; shows ⚠ when update available and watcher on |

- On `config` for a failed/unloaded server, the tabs are hidden (server_config.html:35-42).

**Shared header on every subpage**

- Title: "Server Details – name" plus the UUID.
- `T\panel\parts\details_stats.html`:
  - Status (online/crashed/offline), start time (UTC converted to local with moment), a live uptime counter, server timezone.
  - CPU %, memory, players online/max.
  - Version, MOTD (`JS\motd.js`), server type.
- WS `update_server_details` refreshes the card and re-renders the players table from `players_cache` (189-265, 283).
- `add_server_name()` → `GET /api/v2/servers/{id}` fills the nav badge and sets the update-center ⚠ (287-304).
- `renderPlayers()` is defined here (122-151) and used by admin_controls.

### 4.1 subpage=term → T\panel\server_term.html

- Virtual console: `GET /api/v2/servers/{id}/logs?colors=true` (194-219), then WS `vterm_new_line` appends lines (223-246). The broadcast requires the TERMINAL permission server-side.
- Auto-scroll with a "scroll to bottom" button (44-46, 330-348). Reloads on visibilitychange (353-356).
- Command input with Enter/Send and up/down history → `POST /api/v2/servers/{id}/stdin` with a raw text body (248-328). The input is always rendered; the API requires COMMANDS.
- Start/Restart/Stop buttons, **gated by the Commands perm** (60-106) → `POST …/action/{start_server|restart_server|stop_server}` (123-152). The button set varies with state: installing, updating, starting (waiting_start with delay tooltip), importing.
- WS: `update_button_status` (155-167) and `send_start_reload` → reload (188).

### 4.2 subpage=logs → T\panel\server_logs.html

- Log file select: `GET /api/v2/servers/{id}/logs/files` returns groups `{date, files[], size, active}`. Labels are formatted as date, file count, KB, "(live)" (229-262).
- Viewer: `GET …/logs?colors=true&file=true[&log_file=a,b]` (191-224).
- Line filter words, persisted in localStorage `words` (80-172).

### 4.3 subpage=schedules → T\panel\server_schedules.html

- **SSR** `schedules` (PH:715-718). **⚠API**: `GET /api/v2/servers/{id}/tasks` is a stub (`def get(self, server_id, task_id): pass`, `API\servers\server\tasks\index.py:97-98`). Only `GET /tasks/{id}` and `/tasks/{id}/children` work.
- Table (DataTables): enabled toggle, name, action, command, interval ("every N unit", "reaction + parent", or cron), next run, actions.
  - Toggle → `PATCH /api/v2/servers/{id}/tasks/{sch} {enabled}` (345-358).
  - Edit → /panel/edit_schedule.
  - Run now: confirm with a "cascade children" checkbox → `POST …/tasks/{sch}/run {cascade}` (461-511).
  - Delete: confirm → `DELETE …/tasks/{sch}` (419-459).
- Mobile table with a details modal per task (147-267).
- "Create" → /panel/add_schedule?id=.

### 4.4 subpage=backup → T\panel\server_backup.html

- **SSR** `backups` (model list), `backing_up` (PH:746-759). API: `GET /api/v2/servers/{id}/backups` exists but returns an **unwrapped** payload (`API\servers\server\backups\index.py:131-133`).
- Table: name, "default" badge with explain popup, status badge (Active/Failed/Standby from JSON `status`; click shows the message), location, `max_backups` (column mislabelled "Backup Type"), actions.
  - Edit → /panel/edit_backup.
  - Delete: hidden for the default backup; confirm → `DELETE …/backups/backup/{bid}`.
  - Run now → `POST …/action/backup_server/{bid}/`.
- WS `backup_status` shows a progress bar and reloads at 100% (293-311).
- "New Backup" → /panel/add_backup?id=.
- The file-tree exclusion code at 358-500 is dead on this page; its elements do not exist here.

### 4.5 subpage=files → T\panel\server_files.html plus JS\shared\files.js, JS\shared\upload.js

- Directory listing: `POST /api/v2/servers/{id}/files {page:"files", path, modified_epoch?}`. A 304 means unchanged. Rows are paginated in chunks of 500 with a "load more" row (`files.js:115-150`, `204-276`).
- Columns: checkbox, name/icon, mime, modified, size, permissions (read/write/execute icons), options (⋯).
- `?dir=` query sets the initial path (server_files.html:128-135).
- Breadcrumb navigation (`table-nav`).
- Clicking a directory navigates; middle-click opens a new tab. Clicking a file opens `/panel/edit_file?server_id=&file=` in a new tab (files.js:278-313).
- Context menu (`T\panel\parts\context_menu.html`, `JS\shared\context-menu.js`), built in files.js:325-356:
  - Rename → `PATCH …/files/create {path,new_name}`.
  - Unzip (only for .zip) → `POST …/files/zip/ {folder,proc_id}`, progress via WS `zip_status`.
  - Download → `GET …/files/{path}/download`.
  - Copy / Move: pick destination mode, then `POST …/files/copy/` or `…/files/move/ {file_system_objects}`.
  - Delete: confirm → `DELETE …/files {file_system_objects}`.
- Multi-select toolbar: delete, move, copy (471-552).
- Create directory / create file → `PUT …/files/create {parent,name,directory}` (554-575).
- Upload button and a **drag-and-drop zone** (685-715). Chunked upload (upload.js):
  - Initial `POST /api/v2/servers/{id}/files/upload/` with headers `chunked, fileSize, type, totalChunks, fileName, location, fileId`.
  - Then 10MB chunks with `Content-Range`, `chunkHash` (SHA-256), and `chunkId`.
  - Batches of 30 chunks, at most 2 files in parallel, 2s pause between batches.
  - Progress from WS `upload_process`.
  - A beforeunload guard warns while uploads are active.
- Collapsible bottom "operation status" panel (server_files.html:110-120, files.js:943-953).

### 4.6 /panel/edit_file?server_id=&file= → T\panel\server_file_edit.html plus JS\shared\editor.js (PH:1883-1884)

- **No page-level permission check** in PH; the API enforces the FILES permission.
- Ace editor. Theme is chosen from the html class. Mode is derived from the file extension; unsupported types show a warning.
- Load: `POST …/files {page:"files",path}` returns content and attributes.
- Save (button, Ctrl-S, vim `:w`) → `PATCH …/files {path,contents,modified_epoch,overwrite}`. A 409 opens a conflict prompt: overwrite or re-pull (editor.js:251-300).
- Gear menu: font size (localStorage `font-size`) and keybinds Default/Vim/Emacs/Sublime/VSCode (localStorage `keybind`).
- Unsaved-changes guard.
- Server name badge via `GET /servers/{id}`.

### 4.7 subpage=config → T\panel\server_config.html

- **SSR** `java_versions` (`Helpers.find_java_installs()`, PH:732-745). **No API.** Also `failed`.
- Form → `PATCH /api/v2/servers/{id}` (549-590). Fields and their gates:
  - name
  - **super_user only**: server path (read-only), log_path (not for bedrock), execution_command (read-only for others), server_ip, server_port (with a port hint popover), show_status toggle
  - java_selection (only for minecraft-java/hytale)
  - stop_command, auto_start_delay, shutdown_timeout, ignored_exits, logs_delete_after
  - toggles: auto_start, crash_detection, count_players
- Delete server:
  - Disabled while running (244-247).
  - Normal server: confirm, then choose "delete files" → `DELETE /api/v2/servers/{id}?files=true`, or "keep files" → `DELETE /api/v2/servers/{id}` (310-442).
  - Failed server: `deleteUnloadedConfirm` → plain DELETE (443-490).

### 4.8 subpage=admin_controls → T\panel\server_admin_controls.html plus parts\server_players.html

- **SSR-only**: `banned_players` (from banned-players.json, with `banned_on` formatted) and `cached_players` (PH:923-943). No API; live players arrive only through WS `update_server_details.players_cache`.
- Players table (name, status or last seen):
  - Ban/Kick prompt for a reason, with newlines stripped (70-90).
  - OP/De-OP. Hytale uses `op add|remove`.
  - All actions → `POST …/stdin` as raw console commands.
- Banned table (desktop and mobile) with Unban (`pardon` for Minecraft, `unban` for Hytale). Rows update optimistically (95-138).

### 4.9 subpage=metrics → T\panel\server_metrics.html (PH:761-912)

- **SSR-only**: `chart_data` (adaptive sampling with gap filling, via StatsConverter), `time_range_presets`, `sampling_tiers`, `sampling_fallback_divisor`, `selected_hours`, `max_retention_hours`, `earliest_metrics_date`, `range_mode`, `start_time`, `end_time`, and the `utc_offset` render var.
- Query args: `hours` (or legacy `days`), or `start` and `end` in ISO format.
- **⚠API**: `GET /servers/{id}/history` returns a fixed `get_history_stats(id, 1)` with no range or sampling (`app\classes\shared\server.py:2005-2007`).
- UI:
  - Date-range picker (vanilla-datetimerange-picker) with preset ranges and a custom range; manual text entry with Enter. Selection navigates to a new URL (294-380).
  - Chart.js time chart: CPU % and RAM % on the left axis, RAM GB and Players on the right axis. LTTB decimation. Shift+wheel or drag to zoom, pinch, pan, "Reset zoom" button (426-569, 669-675).
  - Live append from WS `update_server_details`, throttled to the sampling interval, with a sliding window. Skipped in custom-range mode (608-667).
  - Hint popovers.

### 4.10 subpage=webhooks → T\panel\server_webhooks.html

- **SSR** `webhooks` and `triggers` (`WebhookFactory.get_monitored_events()`, PH:913-921). API: `GET /api/v2/servers/{id}/webhook` exists. **Triggers list: no API.**
- Table: enabled toggle → `PATCH …/webhook/{wid} {enabled}`, name, type, translated trigger list, edit → /panel/webhook_edit, delete (confirm → `DELETE …/webhook/{wid}`), test (confirm → `POST …/webhook/{wid}/`).
- Mobile table variant.

### 4.11 subpage=update_center → T\panel\server_update_center.html plus parts\big_bucket_wiz.html

- **SSR**: `server_api` (big bucket alive; no API), `server_types` / `js_server_types` (API: `GET /api/v2/crafty/JarCache`, which **always triggers a cache refresh for superusers**, `API\crafty\exe_cache.py:5-35`), `update`, `update_next_run`.
- Mode select Basic / **Advanced (super_user)** (42-50):
  - Basic (minecraft-java only): current update URL (locked), cascading selects category → type → version. Versions below 1.8 show an "unsupported" popover. Superuser gets a cache refresh button → `GET /api/v2/crafty/JarCache`.
  - Advanced: executable_update_url.
- Executable filename (super_user only).
- Info card:
  - `update_watcher` toggle (java only) → `PATCH /api/v2/servers/{id}/update/config/ {update_watcher}`.
  - Up-to-date / new-version text, next check time, current executable.
  - "Update" → `POST …/action/update_executable`, with a progress bar.
- Submit:
  - Basic → `PATCH …/update/config/ {category,type,version}`, then `PATCH /servers/{id}/ {executable}`.
  - Advanced → `PATCH /servers/{id}/ {executable_update_url,executable}` (222-288).
- WS: `remove_spinner` → go to the terminal (290), `backup_status` (pre-update backup progress, 295).

### 4.12 Schedule / backup / webhook editors (separate /panel pages that reuse the server tab bar)

**/panel/add_schedule?id=, /panel/edit_schedule?id=&sch_id= → T\panel\server_schedule_edit.html**

- Handler: PH:1260-1422. Requires the SCHEDULE perm; edit also runs `check_server_id`.
- SSR data: `schedule` defaults or model, `schedules` (for the parent select), `backups` (for the policy select), `children`, `parent`, `difficulty`.
- Fields:
  - name
  - mode: basic / cron ("advanced") / reaction
  - action: start / restart / stop / backup / command
  - backup policy (`action_id`) when the action is backup
  - interval and unit (days/hours/minutes); time of day when the unit is days
  - command (custom only)
  - cron string
  - reaction: delay offset and parent schedule
  - enabled, one_time
- Right column: children list with links.
- Save → `POST /api/v2/servers/{id}/tasks/` or `PATCH …/tasks/{sch_id}`. For non-command actions the command is set to `"{action}_server"`. `interval_type` is `"reaction"` for reactions and `""` when a cron string is set (254-355).

**/panel/add_backup?id=, /panel/edit_backup?id=&backup_id= → T\panel\server_backup_edit.html**

- Handler: PH:1424-1552. Requires the BACKUP perm.
- **SSR data**: `backup_config` (API: `GET …/backups/backup/{bid}`), `backup_list` (from `backup_mgr.list_backups`). **⚠API**: `GET …/backups/backup/{bid}/files` lists `*.zip` only, so snapshot backups are missed (`API\servers\server\backups\backup\index.py:465-474`). Also `exclusions` (paths made relative), `backing_up`, `backup_path`.
- "Backup now" → `POST …/action/backup_server/{bid}`. Hidden while a backup is running.
- New backup only: type radio **Full Zip Vault / Snapshot**. Snapshot shows a BETA ribbon and a disclaimer.
- Fields:
  - name (with default badge)
  - **backup_location (super_user only)**
  - max_backups
  - toggles: compress, shutdown
  - "before" / "after" command fields, each behind a checkbox
  - exclusions: "click to choose" opens a modal file tree with checkboxes → `POST /api/v2/servers/{id}/files[/{backup_id}] {page:"backups",path}`
- Save → `POST …/backups/` (new) or `PATCH …/backups/backup/{bid}/`.
- Current backups table:
  - Download (not for snapshots) → `GET …/backups/backup/{bid}/download/{file}`.
  - Delete → `DELETE …/backups/backup/{bid}/files/ {filename}`.
  - Restore: snapshot restores in place; zip asks in-place vs replace → `POST …/backups/backup/{bid}/ {filename,inPlace}`, then go to the terminal.
- Excluded-dirs list.
- WS `backup_status`.

**/panel/add_webhook?id=, /panel/webhook_edit?id=&webhook_id= → T\panel\server_webhook_edit.html**

- Handler: PH:1156-1258. Requires the CONFIG perm or superuser. **No `check_server_id`.**
- **SSR-only**: `providers` (`WebhookFactory.get_supported_providers()`) and `triggers`. Neither is in `/api/v2/jsonschema`.
- Fields: type, name, url, bot_name, trigger multi-select (bootstrap-select), body (Jinja2, with a docs link), color, enabled.
- Save → `POST /api/v2/servers/{id}/webhook/` or `PATCH …/webhook/{wid}`. On edit, color is dropped unless the type is Discord.

---

## 5. Panel settings

The settings sub-tabs (`T\panel\parts\crafty_config_list.html`, mobile `m_crafty_config_list.html`) are shown **only if `data['superuser']`**: Panel Config, Config.json, Custom Login.

**/panel/panel_config → T\panel\panel_config.html** (PH:947-1021)

- **No handler gate**; any logged-in user can open it.
- **SSR-only**:
  - `users`: all users for a superuser, otherwise `user_query`.
  - `managed_users`.
  - `auth-servers`: per-user authorized server names. **No API.**
  - `user-roles`.
  - `roles` / `managed_roles` / `assigned_roles`.
  - `role-servers`.
  - `servers_dir`: **⚠API** `GET /crafty/config/servers_dir` returns roles by mistake.
- Available APIs: `/users` (public attributes: user_id, created, username, enabled, superuser, lang; managed users for USER_CONFIG), `/roles` (requires ROLES_CONFIG), `/roles/{id}/users`, and `/roles/{id}/servers` (**superuser only**).
- Users table: name, enabled, allowed servers, roles. Actions:
  - Shield icon (only if the user has TOTP) → renew backup codes, `GET /api/v2/users/{id}/totp/recovery/renew/` (`JS\shared\backupCodes.js`).
  - Change username → `PATCH /api/v2/users/{id} {username}`.
  - Change password dialog with a match check → `PATCH {password}` (`JS\shared\userSettings.js`).
  - Edit → /panel/edit_user.
- "New User" and "New Role" links.
- Roles table: name, servers, users, edit.
- **Admin controls (superuser and not docker)** (270-316):
  - Global server directory → `PATCH /api/v2/crafty/config/servers_dir {new_dir}`, progress via WS `move_status` (372-410).
  - Help text `serverConfigHelp.perms`.

**/panel/config_json → T\panel\config_json.html** (PH:1023-1048, `exec_user.superuser` only)

- **SSR-only**:
  - `config-json`: settings grouped by `CONFIG_CATEGORIES` general / security / logs / monitoring / miscellaneous (`app\classes\helpers\helpers.py:55-145`, `634-655`).
  - `availables_languages`, `all_languages`.
  - `all_partitions`.
- **⚠API**: `GET /api/v2/crafty/config` returns **roles**, not the config (`API\crafty\config\index.py:273-313`).
- UI: collapsible sections. Labels come from `translate('configJson', key)`. Inputs by setting type:
  - `language`: select with humanized names.
  - `disabled_language_files`: multi-select.
  - `monitored_mounts`: multi-select of partitions.
  - `time_range_presets`: {hours, label} table editor.
  - `sampling_tiers`: {max_hours, sample_rate} table editor.
  - lists: comma-separated textarea.
  - bools: True/False radios.
  - ints: number inputs.
  - strings: text inputs.
- Submit → `PATCH /api/v2/crafty/config/` with the whole object (255-331).

**/panel/custom_login → T\panel\custom_login.html**: see section 13.

**/panel/activity_logs → T\panel\activity_logs.html**

- Handler: no gate. Menu link: superuser only.
- `GET /api/v2/crafty/logs/audit` (superuser) → DataTables with Time, Username (links to edit_user unless system), Action, Server ID, IP. Rotating logo while loading.

---

## 6. Users and roles

**/panel/add_user, /panel/edit_user?id= → T\panel\panel_edit_user.html** (PH:1092-1154, 1554-1645)

- Gates:
  - add: USER_CONFIG.
  - edit: USER_CONFIG unless editing yourself. You must be the manager, a superuser, or the user (the redirect at 1630 has no `return`).
- **SSR-only**:
  - `languages`: translation files minus `disabled_language_files`, current language first. **No API.**
  - `themes`. **No API.**
  - `permissions_all`, `permissions_list`, `quantity_server`. Partly covered by `/users/{id}/permissions` {permissions, counters, limits}.
  - `manager`, `users` (manager select), `roles`, `super-disabled`, `passkey_enabled`. **No API** for passkey_enabled; the login options endpoint is only a proxy.
- Tabs: Config / API Keys / TOTP / Passkeys. Passkeys only if `passkey_enabled`. The extra tabs are hidden for a new user.
- Form:
  - username (new only), password + repeat (new only)
  - Gravatar email
  - language (languages containing `incomplete` are disabled)
  - theme
  - **manager (superuser only)**
  - role membership table: shown if superuser, already a member, or the role's manager; checkbox editable for superuser or role manager
  - **Crafty permissions table with quantity limits (superuser only)**
  - enabled
  - superuser: disabled unless the editor is a superuser and not editing themselves; confirm dialog
  - hints
- Save:
  - First `GET /api/v2/users/@me`.
  - Then `POST /api/v2/users/` or `PATCH /api/v2/users/{id}` with {email, lang, theme, manager, roles[], permissions[{name,quantity,enabled}], enabled, superuser, hints, password}.
- Side card: created, last login, last update, last IP, manager. Buttons: change username, change password (userSettings.js).
- Delete: disabled for new users and superusers → `DELETE /api/v2/users/{id}`.
- Legacy `GET /panel/remove_user?id=` deletes directly (PH:1743-1781). It is not used by any template.

**/panel/edit_user_apikeys?id= → T\panel\panel_edit_user_apikeys.html** (PH:1647-1673; yourself or superuser)

- **SSR**: `api_keys` (API: `GET /users/{id}/key`), `server_permissions_all` / `crafty_permissions_all` (enum names only; **no API**), `user_crafty_permissions`.
- Keys table: name, created, full access, permission masks.
  - Delete → `DELETE /users/{id}/key/{kid}`.
  - "Get token" → `GET /users/{id}/key/{kid}`, token shown in a bootbox.
- Create form: name, 8 server-permission checkboxes, crafty-permission checkboxes (disabled where the user lacks the permission), full_access → `PATCH /api/v2/users/{id}/key/ {name,server_permissions_mask,crafty_permissions_mask,full_access}`.

**/panel/edit_user_otp?id= → T\panel\panel_edit_user_otp.html** (PH:1675-1700; yourself or superuser)

- **SSR** `totp` list. API: `GET /users/{numeric id}/totp`; the `@me` form of this route does not exist.
- Renew recovery codes (backupCodes.js).
- New OTP → `POST /users/{id}/totp/` → modal with a name input, QR code (qrcode.min.js, otpauth URI, issuer "CraftyController"), the secret, and a verify box → `POST /users/{id}/totp/{tid}/verify/ {name,totp}`. On success it shows backup codes with a copy button, then **clears the `token` and `_xsrf` cookies**, forcing a re-login (183-256).
- Delete: confirm → `DELETE /users/{id}/totp/{tid}/`.

**/panel/edit_user_passkey?id= → T\panel\panel_edit_user_passkey.html** (PH:1702-1741)

- Requires passkeys to be enabled; yourself or superuser.
- **SSR** `passkeys` (API: `GET /users/{id}/passkeys`), `disable_password_auth`, `has_passkeys`.
- Table: name (cloud icon if backed up), device type, created, last used, delete (confirm → `DELETE /users/{id}/passkeys/{pid}/`).
- New passkey: `POST /users/{id}/passkeys/` → `navigator.credentials.create` → prompt for a name → `POST /users/{id}/passkeys/{challenge_id}/verify/ {name,credential}`.
- "Disable password auth" toggle, disabled when there are no passkeys → `PATCH /users/{id}/ {disable_password_auth}`.

**/panel/add_role, /panel/edit_role?id= → T\panel\panel_edit_role.html** (PH:1783-1876)

- Gates: ROLES_CONFIG. Edit also requires being the role's manager or a superuser.
- **SSR**: `role` (with servers; API `/roles/{id}`), `servers_all` (servers the user can see), `permissions_all` (server enum names; **no API**), `permissions_dict` (API `/roles/{id}/servers`, superuser only), `user-roles`, `users`, `role_manager`.
- Form:
  - name
  - **manager (superuser only)**
  - mfa_required switch
  - server × permission matrix (rotated headers). The per-server "access" checkbox enables that row's permission boxes (313-325).
- Save → `POST /api/v2/roles/` or `PATCH /api/v2/roles/{id}` with {name, manager, mfa_required, servers:[{server_id, permissions:"01010101"}]}.
- Role users table, info card, delete → `DELETE /api/v2/roles/{id}`.

---

## 7. Server creation wizards (ServerHandler, /server/<page>)

**Gate** for every wizard page: superuser or `crafty_perms.can_create_server` (SH:154-212).

**SSR data** (SH:98-152):

- `experimental`, `windows`, `online` (a live `Helpers.check_internet()`; **no API**), `server_api` (big bucket alive; **no API**).
- `roles`: all roles for a superuser, otherwise the user's own roles. `/api/v2/roles` requires ROLES_CONFIG, so non-privileged users have **no API** for this.
- The usual shell data.

Wizard tabs: Minecraft-Java, Bedrock, Hytale, and Steam-CMD (only if `experimental`).

**/server/step1 → T\server\wizard.html (Java)**

- Form is disabled unless `server_api` and `online`. Alerts for big-bucket-down or no internet.
- Fields: big-bucket cascade (`server_types`), name, min/max memory (GB), port (25565), roles multi-select (`T\panel\parts\role_select.html`, bootstrap-select).
- Submit → `POST /api/v2/servers/` with `{name, roles, monitoring_type:"minecraft_java", minecraft_java_monitoring_data:{host,port}, create_type:"minecraft_java", minecraft_java_create_data:{create_type:"download_jar", download_jar_create_data:{category,type,version,mem_min,mem_max,server_properties_port}}}`. Redirects to the dashboard.
- Import zip:
  - Chunked upload (type `import`) → `/api/v2/servers/import/upload/`.
  - Choose the root folder inside the archive via a modal tree → `POST /api/v2/import/archive/select {file_name,local_path}` (`JS\shared\root-dir.js`).
  - Then jar, memory, port, roles → `import_server_create_data {archive_name, archive_internal_path, jarfile, …}`.

**/server/bedrock_step1 → T\server\bedrock_wizard.html**

- Minecraft EULA and Microsoft privacy confirm → `download_exe {agree_to_eula:true}`. Monitoring port 19132.
- Import zip with port 19132.

**/server/hytale_step1 → T\server\hytale_wizard.html**

- Hytale EULA confirm → `download_exe {agree_to_eula, mem_min, mem_max}`. Port 5520.
- Import zip.
- On success, redirects to the new server's terminal. The auth link arrives via WS `hytale_auth`.

**/server/steam_cmd_step1 → T\server\steam_wizard.html**

- Requires the `experimental` setting.
- App select, filtered by OS, from SSR `servers` (SteamCMD games) and `os`.
- Superuser refresh → `GET /api/v2/crafty/SteamCache/` (superuser only).
- Submit → `steam_cmd` / `download_exe {app_id,…}`, port 27015.
- EXPERIMENTAL ribbon.

---

## 8. Other /panel pages

- **/panel/credits → T\panel\credits.html**: **SSR-only**. The server fetches `https://craftycontrol.com/credits-v2` and falls back to the local credits cache (PH:420-455). Shows development, support and retired staff cards with tag links, a patrons table, a translators table, and `lastUpdate` (😿 when the local cache is used).
- **/panel/contribute → T\panel\contribute.html**: static Ko-fi, Patreon and Discord links.
- **/panel/wiki → T\panel\wiki.html**: iframe of https://docs.craftycontrol.com.
- **/panel/loading → T\panel\loading.html**: shown while Crafty starts. Rotating logo. WS `update {section, server?}` maps to the `startup.*` translation strings. WS `send_start_reload` → dashboard after 5s. **No API** for the "starting" state.

---

## 9. WebSocket events, emitters and listeners

WS client pages are keyed on `page` = pathname and `page_query_params`, **captured once at connect** (`websocket_handler.py:95-103`). **SPA implication**: either reconnect on every route change with matching page/params, or add a subscribe message on the server; `on_message` is currently a no-op.

| Event                 | Emitted by / scope                                                                                    | Listened in                                    |
| --------------------- | ----------------------------------------------------------------------------------------------------- | ---------------------------------------------- |
| update_host_stats     | tasks.py:856, 877 → page `/panel/dashboard`                                                           | dashboard:949                                  |
| update_server_status  | server.py:1648 → page `/panel/dashboard` (all servers, no perm filter)                                | dashboard:1007                                 |
| update_server_details | server.py:1616 → `/panel/server_detail` + id                                                          | details_stats:283, metrics:620                 |
| vterm_new_line        | server.py:215, installers hytale.py:104 and modded.py:60 → server_detail + id, requires TERMINAL      | term:245                                       |
| backup_status         | backup_mgr.py:318, 369 and file_helpers.py:608-670 → server_detail + id and `/panel/edit_backup` + id | backup:294, backup_edit:438, update_center:295 |
| download_progress     | file_helpers.py:318 → page server_detail (no id)                                                      | base:329                                       |
| update_button_status  | server.py:1441, 1473 → user + server_detail page (server.py:1483 has user/page arguments swapped)     | dashboard:991, term:155                        |
| remove_spinner        | server.py:1510 → server users                                                                         | update_center:290                              |
| send_start_reload     | server.py, import_helper.py → server users or user                                                    | dashboard:985, term:188, loading:65            |
| send_error            | server.py (start errors), backup_mgr.py, import_helper.py, tasks.py:205, main_controller.py:1128      | base:313                                       |
| send_start_error      | server.py:670                                                                                         | **no listener**                                |
| send_eula_bootbox     | server.py:794 → user                                                                                  | base:404                                       |
| hytale_auth           | installers\hytale.py:122                                                                              | base:342                                       |
| notification          | many places (management.py:160 audit, backups, updates, ws auth failure)                              | base:571                                       |
| support_status_update | main_controller.py:318, 356 → user                                                                    | base:385                                       |
| zip_status            | file_helpers.py:739 → user                                                                            | base:600                                       |
| upload_process        | `API\crafty\upload\index.py:242` → user                                                               | upload.js:247                                  |
| move_status           | main_controller.py:1096-1193 → page `/panel/panel_config`                                             | panel_config:373                               |
| update                | servers_controller.py:234-238 (startup progress)                                                      | loading:58                                     |
| send_logs_bootbox     | never emitted                                                                                         | base:394                                       |

---

## 10. localStorage keys and URL parameters to preserve

- localStorage: `crafty-sidebar-expanded`, `notif-count`, `words` (log filter), `font-size`, `keybind` (editor).
- URL parameters:
  - server_detail: `id`, `subpage`, `dir`, `hours`/`days`, `start`/`end`.
  - edit_file: `server_id`, `file`.
  - edit_schedule: `sch_id`.
  - edit_backup: `backup_id`.
  - webhook_edit: `webhook_id`.
  - error page: `error`.

---

## 11. API gaps and bugs (SSR-only data the SPA will need endpoints for)

**Broken endpoints**

1. `GET /api/v2/crafty/config`, `GET /crafty/config/customize` and `GET /crafty/config/servers_dir` all return the roles list (copy-paste bug). Locations: `API\crafty\config\index.py:273-313`, `377-417`, and `API\crafty\config\server_dir.py:17-57`. Config.json, login customization and servers_dir cannot be read.
2. `GET /api/v2/servers/{id}/tasks` is a `pass` stub with the wrong signature (`API\servers\server\tasks\index.py:97-98`). The schedules list cannot be read.
3. `GET /servers/{id}/backups` response is not wrapped in the usual `{status, data}` envelope. Backup files `GET` lists `*.zip` only.
4. `GET /servers/{id}/history` has no range or sampling. Metrics presets, tiers and earliest date are SSR-only.
5. `GET /api/v2/servers/status` (public) lacks `server_name`.
6. `GET /users/@me/totp` list route is missing (only the numeric id works). `/roles/{id}/servers` is superuser-only while role managers need it.

**Missing endpoints (no API at all)** 7. The current user's effective permissions on a server (used to gate tabs and buttons). 8. Public (unauthenticated) branding: `brand` full/square, `background`, `login_opacity`, embed settings. 9. Lists: themes, languages (available, all, humanized names), disk partitions, Java installs, webhook providers and triggers, server/crafty permission enum names. 10. Panel runtime state: is Crafty starting, first login, docker, server timezone, internet reachable, big bucket alive, failed/unloaded servers, `update_available` flag, API-key session info, MFA claim. 11. Per-server state: banned players, player cache, importing, waiting_start, update next run. 12. Credits data. 13. Per-user authorized server list (panel_config). 14. Roles for the wizard role-select when the user lacks ROLES_CONFIG. 15. Translations (see section 12). 16. Logout (only `GET /logout`).

**Other constraints and quirks** 17. OG meta for /login and /status needs server-side HTML injection. 18. `GET /crafty/JarCache` always refreshes the cache for superusers.

---

## 12. i18n

- **Files**: `app\translations\*.json`. There are **28 languages**: cs_CS, de_DE, en_EN, es_ES, fi_FI, fr_FR, fy_NL, he_IL, hr_HR, hu_HU, id_ID, it_IT, ja_JP, ko_KR, lol_EN, lv_LV, nl_BE, nl_NL, pl_PL, pt_BR, ru_RU, sr_RS, th_TH, tr_TR, uk_UA, vi_VN, zh_CN, zh_Hant.
- `humanized_index.json` holds `{"language": {code: display name}}`, but only 16 codes; the others fall back to showing the raw code.
- **Format**: `{section: {key: value}}`. en_EN has 37 sections and 788 keys.
  - Sections: 404, accessDenied, apiKeys, base, configJson, credits, customLogin, dashboard, datatables, error, footer, login, notify, offline, otp, passkey, panelConfig, rolesConfig, serverBackups, serverConfig, serverConfigHelp, serverDetails, serverFiles, serverFilesEditor, serverMetrics, serverPlayerManagement, serverScheduleConfig, serverSchedules, serverStats, serverTerm, serverUpdate, serverWizard, sidebar, startup, userConfig, validators, webhooks.
  - Values are strings. Exceptions: `datatables.i18n` is an object (the DataTables language block) and `serverConfigHelp.perms` is an array (joined with "\n").
  - Some strings contain HTML (templates render them with `{% raw %}`) and Python `{}` placeholders (`.format()` on the server).
- **Completeness varies**: hu_HU has 190 keys, sr_RS 328, fi/fy/hr about 400. de/fr/ru/uk/vi have 757. nl_NL, tr, zh_CN, zh_Hant have 785.
- **Mechanism**: `Translation.translate(page, word, language, error=True)` in `app\classes\shared\translation.py:23-43`. It falls back to en_EN. A dict value is returned as a JSON string and a list is joined with newlines. A missing key returns "Error while getting translation", or the key itself when `error=False`. The translator is passed to templates as `translate` (PH:1892).
- **Language used**:
  - Authenticated pages: the user's `lang` (PH:389).
  - Public pages: the global `language` setting.
  - `lang_page` maps `xx_YY` to `xx-YY` and any `*_EN` to `en`, for `<html lang>` (`app\classes\helpers\helpers.py:1483-1490`).
  - The `disabled_language_files` setting removes languages from the lists; filenames containing `incomplete` are shown disabled.
- API error messages (`validators.*` etc.) and WS notifications are **already localized server-side** in the user's language.
- **There is no API endpoint serving translations.** The SPA needs a new endpoint such as `/api/v2/crafty/lang/{code}` plus a language list, or it must bundle the JSON at build time and track the disabled-language setting.

---

## 13. Branding and customization

- `controller.cached_login` is `"login_1.jpg"` or `"custom/<file>"`. `controller.cached_brand` = `resolve_brand_paths(get_brand_settings())` gives `{full, square}` static paths, with stock `logo_long.svg` and `crafty-logo-square.svg` as fallbacks (`app\classes\shared\main_controller.py:101-120`, `app\classes\helpers\brand_helpers.py:12-28`).
- Stored in the CraftySettings table: login_photo, login_opacity, og_enabled/title/description/image/color, logo_full, logo_square (`app\classes\models\management.py:233-313`).
- Where brand is used: top-bar logos, toast icon, login, status, 404, error, denied, offline, loading and activity pages.
- **/panel/custom_login → T\panel\custom_login.html** (PH:1050-1090, superuser)
  - **SSR-only data**: `backgrounds` (current, login_1.jpg, files in `images/auth/custom`), `background`, `login_opacity`, `embed_settings`, `embed_images` (`images/embed`), `brand_settings`, `logo_full_images` / `logo_square_images` (`images/logos/full|square`).
  - Reusable image picker (`T\panel\parts\image_picker.html`): thumbnail with drag-and-drop, select, delete, upload. Changes apply instantly (451-546).
    - Login background: `PATCH/DELETE /api/v2/crafty/config/customize {photo, opacity}`, upload to `/api/v2/crafty/admin/upload/`. Recommended 1920x1080.
    - Full logo (512x128) and square logo (256x256): `PATCH/DELETE /api/v2/crafty/config/logos {logo_full|logo_square}`, uploads to `/api/v2/crafty/admin/upload/logo_full/` and `/logo_square/`.
    - Embed image (1200x630): `PATCH/DELETE /api/v2/crafty/config/embed {og_image}`, upload to `/api/v2/crafty/admin/upload/embed/`.
  - Opacity slider: live preview, debounced save through `/config/customize` (548-570).
  - Live login preview with background, logo and opacity (130-231).
  - Embed (OG) form: enabled, title, description, color. Shows an unsaved hint. Apply → `PATCH /api/v2/crafty/config/embed` (578-602).
  - Screen-reader live region for status messages.
- **Themes**: CSS files in `static\assets\css\themes\` (dark, default, light, ronald), each defining `:root.<theme>`. Plus the internal `anti-lockout.css`. The user's `theme` field becomes the `<html>` class. The theme list is **SSR-only**.
- **Status page**: see section 2, including the per-server `show_status` flag (a super_user-only toggle on the config tab).
- **PWA**: `static\assets\crafty.webmanifest` plus the service worker, a workbox stub.

---

## 14. First-run setup and anti-lockout

- **`T\setup\setup1.html` is orphaned or dead.** No handler renders it. It uses gettext `_()` and posts to `/login`, and PublicHandler.post just redirects (`public_handler.py:73-74`). It was obsoleted by commit d97a7929. There is no first-run wizard UI.
- **Actual first run** (`main.py:388-419`, `app\classes\shared\main_models.py:17-45`):
  - A fresh install creates superuser `admin` with email `default@example.com`.
  - The password is random and written to `app/config/default-creds.txt` (chmod 600).
  - Alternatively, `app/config/default.json` can supply username and password if the password meets the minimum length. That file is deleted if `delete_default_json` is set (`helpers.py:1412-1425`).
  - `controller.first_login=True` triggers the one-time survey bootbox on the dashboard for "admin" (PH:461-463, dashboard.html:26-46).
  - If someone types the literal `app/config/default-creds.txt` as the password, the login error adds a hint (`API\auth\login.py:328-332`).
  - edit_user blanks the email field when it is `default@example.com` (PH:1642-1643).
- **Anti-lockout**:
  - Login "Forgot password" → `GET /api/v2/crafty/resetPass` (unauthenticated, logged to auth_tracker) (`API\crafty\antilockout\index.py`).
    - Creates superuser `anti-lockout-user` with a random password **printed only to the server console**, theme `anti-lockout`.
    - Auto-deleted after 1 hour (scheduler job), on `/logout`, and on every restart (`app\classes\controllers\users_controller.py:427-454`, `public_handler.py:54-56`, `main.py:445-446`).
    - Returns 425 if recovery is already active.
  - While logged in as that user:
    - Every `/panel/*` page is forced to panel_config (PH:347-348). `/server/*` redirects there too (SH:95-96).
    - Blue warning banner with a logout link (base.html:580-591).
    - Red/orange anti-lockout theme.
    - MFA enforcement is skipped (`base_handler.py:294`).
  - The name cannot be created or renamed-to (`users_controller.py:236, 313`) and is hidden from user queries (`app\classes\models\users.py:110`).

---

## 15. Dead or buggy code: do not port

- `base.html:186-197`: `$.get("/ajax/announcements")` on a non-existent `#notificationDropdown`.
- `config_json.html:423-468`: `/ajax/clear_comm`, `/ajax/delete_photo`, `/ajax/select_photo`, and `.show_button`. The same `.show_button` handler is in custom_login.html:353-361.
- `server_backup.html:358-500`: tree/exclusion code whose elements are absent on that page.
- `JS\shared\editor.js:211`: Tornado `{% raw translate %}` syntax inside a static JS file, so the literal template text is displayed.
- `send_logs_bootbox` listener with no emitter. `send_start_error` emitter with no listener.
- `blank_page_template.html` and `setup\setup1.html` are unused.
- PH:1386-1394: an always-true condition on `schedule.action`.
- PH:1491 and 1550: backup permission failures redirect with the schedule error text.
- The Metrics tab link is shown without a permission check, while the handler requires LOGS.
